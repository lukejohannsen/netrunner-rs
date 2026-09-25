//! The board: the human's masked view of the match, drawn; a control bar
//! of the basic actions, greyed when the engine does not list them; the
//! cards and the zones as the way into everything else — a click opens a
//! sheet of what may be done there, and never acts by itself; the
//! prompt's decisions under the prompt; and, when the person turns them
//! on, the flat panel of every legal action and the match log.
//!
//! **Where the match is.** On its own thread, behind the [`ActiveMatch`]
//! the new-game form left: `poll` drains its messages once a frame into
//! `models::game::Game` intents, and a chosen entry goes back through
//! `MatchHandle::submit`, unfiltered. The screen never holds a
//! `GameState` and never re-derives legality; it draws the view and
//! hands back what the person chose from the engine's own list.
//!
//! **Layout, and the rule it obeys: the board never scrolls.** The
//! window is fullscreen and the board fits it — a static card size put
//! the person's own hand below the fold twice, and a scroll container
//! is not a fix for a card table. `models::layout::face_width` computes
//! the card width the window has room for from the window's size, the
//! chair and the number of servers — never from what is installed, so
//! nothing the opponent does moves a card — `fit` recomputes it whenever
//! one of those changes, and a row that is still too wide overlaps its
//! cards like a held hand (`layout::step`) rather than wrapping or
//! scrolling. The board runs the window's full height beside one right
//! column: nothing above the opponent's avatar bar and nothing under the
//! person's hand. Top to bottom on the board: the opponent's avatar bar
//! on the window's top edge (their hand is not drawn: its count is on
//! the bar or the HQ header); their area and the person's, meeting at
//! the ICE; the control bar (`board::Control::for_side`, one button
//! each, always in the same place), then the person's avatar bar and the
//! top of their hand on the window's bottom edge.
//! The right column is the status line with Quit and the gear, the phase
//! panel (`board::phase`, hidden with L; a press on it or T opens the
//! timing chart, `board::timing`), the Runner's identity while a
//! run is on (or the ICE, while the run encounters one), the rail and
//! the log. The opponent's side is
//! drawn at `layout::OPPONENT_SCALE` of the person's own. Each of the
//! Corp's servers is a column with its plate on the Corp's edge of the
//! table and its ICE as strips out toward the Runner — bars rather than
//! rotated cards, since a rotated `UiTransform` is laid out as its
//! unrotated box and would overlap its neighbours — a fixed number of
//! them, with a "+N" strip for what does not fit and the whole server in
//! its stack sheet (`layout::ServerWindow`). The Runner's rig is three
//! rows reserved whether or not anything is in them, taking the height
//! the window has spare, with the stack and the heap on the Runner's
//! avatar bar. The rail is the prompt (`board::Prompt`, the card's
//! own words), the decisions the prompt is asking
//! (`ActionMap::decisions`), the flat panel if the play helper is on,
//! and the log if the play history is. Everything under the board root
//! is respawned when the view moves, which is at most once per applied
//! action, and when the fit changes; the bar, the rail and the overlay
//! are respawned on their own, so a sheet opening does not redraw the
//! board.
//!
//! **Sheets are for reading.** A card's sheet is the Large face — the
//! picture, or the text layout while no picture is cached — and, for an
//! installed card, its state beside it; a zone's sheet is its contents
//! where the viewer may see them — Archives as every card for the Corp
//! and as the face-up ones with backs for the rest for the Runner, the
//! heap, the Corp's own HQ, a remote's ICE and root — each face a button
//! that reads the card over the sheet. R&D and the stack are backs and a
//! count: a deck's order is never shown, even to its owner.
//!
//! **A click is a menu above the card; a secondary click is its sheet.**
//! The primary button on a card or a zone opens its actions as a small
//! panel sitting just above the node that was clicked, centred on it, so
//! the card stays in view under its own menu (`models::game::Menu`,
//! anchored by the node's laid-out box, not the pointer — a menu at the
//! pointer landed somewhere different on every click). The right button,
//! or the primary with Ctrl or Cmd held (a Mac's one-button click),
//! opens the sheet to read. `Interaction` reports only the primary
//! button, so `board_click` reads the right button from
//! `ButtonInput<MouseButton>` and takes the target from the node the
//! focus system marks hovered — every card, ICE bar and header is a
//! `Button`, which blocks, so the topmost is the one under the pointer.
//! A primary click that lands on nothing of the menu's closes it, and
//! so does the board moving; neither opens through an overlay, whose
//! ground passes hovers to the board beneath.
//!
//! **A hand card is dragged into place** (`models::drag`). A press on a
//! hand card is armed rather than acted on: released where it started it
//! is the click that opens the card's menu, and released after six pixels
//! of travel it is a drag, which drops the card between the two cards the
//! pointer is over (`models::game::HandOrder`, the person's own order for
//! a zone the rules give no order). Every other target still acts on the
//! press. `drag_hand` reads the pointer from `CursorMoved` rather than the
//! window, so the headless tests can drive it.
//!
//! **Keys are the buttons pressed another way** (`models::shortcuts`):
//! `shortcuts` reads the keyboard messages by the character a key types,
//! hands the model what it can answer, and acts itself only on the two it
//! cannot — the hovered card's menu and reading, and the play helper
//! setting. `?` or F1 lists them over the board.
//!
//! **Highlights come from `board::diff`.** After a redraw, every install
//! and hand card a `Transition` names is outlined for that redraw, and
//! nothing is ever inferred from the previous frame's nodes. §4 turns
//! the same transitions into movement and sound.
//!
//! **A run is shown a beat at a time.** The match's messages go through
//! `models::pace::Pacer` rather than straight into the model: in a run
//! each event that moves the run is released on its own beat, the
//! model's `RunTrail` observes it and the right column's run panel
//! redraws (`side_panels`, without the board), and the message itself
//! — the board — lands last. The column under run keeps its border in
//! the Runner's colour; a line to it was drawn and dropped the same day,
//! on the person's word that it was ugly.
//!
//! **There is no run lane.** A row of chips between the two areas —
//! the server, each ice, the access, the outcome — was the board's for
//! a run until 24 September 2026, when the person asked for it gone:
//! the phase panel (L) says where the run is and the encounter panel
//! which ICE and which subroutines, so the lane was the same facts a
//! third time, and its ICE chips were a second box for the ICE's own
//! click, which the actions menu re-anchored to (`place_menu`) — the
//! Corp's rez menu opened over the lane, not the ICE.

use bevy::input::keyboard::{Key as BevyKey, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use netrunner_client::access::Access;
use netrunner_client::board::action_map::server_name;
use netrunner_client::board::{facts, hud, Affordance, Control, Encounter, Pile, Prompt, Target, Token, TokenKind, Transition, Zone};
use netrunner_client::board::opening::Opening;
use netrunner_client::card_face::Face;
use netrunner_client::standing::Answer;
use netrunner_core::dsl::{CardId, CardType};
use netrunner_core::rules::{GamePhase, InstallId, InstallSlot, PendingDecision, PlayerAction, RunPhase, ServerId, Side, SubroutineStatus};
use netrunner_core::view::{ClientView, ServerView};

use crate::card_images::{CardImages, WantsImage};
use crate::core::{ClientCore, Notices};
use crate::models::game::{Anchor, Game, Intent, MatchMessageRef, Outcome};
use crate::board_art::{self, BoardArt};
use crate::models::layout::{self, Counts, Depth};
use crate::models::pace::{Beat, Pacer};
use crate::models::settings::{self as settings_model, Row};
use crate::models::shortcuts::{self, Shortcut};
use crate::nav::{screen_root, Captures, InputCaptured, Navigate};
use crate::models::lesson::LessonBoard;
use crate::screens::new_game::{ActiveMatch, LastGame};
use crate::screens::replay::{ActiveReplay, OpenReplay, ReplayClick};
use crate::screens::settings::{self as settings_screen, Control as SettingsControl};
use crate::screens::AppScreen;
use crate::skin::{self, Drawn, Slot};
use crate::table;
use crate::theme::{size, Theme};
use crate::widgets::card_face::{spawn_back, spawn_face, FaceSize};
use crate::widgets::{self, anchor_of, ButtonKind, Pressed};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // `CursorMoved` is `WindowPlugin`'s, and the headless tests run
        // without one; registering it here is a no-op when the window
        // plugin already has, and lets a test drive a drag by message.
        app.add_message::<CursorMoved>()
            .init_resource::<Pending>()
            .init_resource::<table::LastTable>()
            .init_resource::<Pointer>()
            .init_resource::<AvatarCrops>()
            .add_observer(open_a_logged_name)
            .add_systems(OnEnter(AppScreen::Game), spawn)
            .add_systems(OnExit(AppScreen::Game), leave)
            .add_systems(Update, (poll, autoplay, escape.in_set(Captures), board_click, drag_hand, shortcuts, controls, fit, board_pictures, side_panels, redraw, fade_ghosts, raise_hand, table_guide).chain().run_if(in_state(AppScreen::Game)))
            // Its own registration rather than a link in that chain: it
            // has no ordering requirement against any of them, and adding
            // a system to an existing `.chain()` reorders everything after
            // it (§4n moved `button_feedback` that way and broke twelve
            // board tests).
            .add_systems(Update, shadows.run_if(in_state(AppScreen::Game)))
            // The avatars' crops, on their own line for the same reason:
            // what they fill is read by the next redraw, whenever it is.
            .add_systems(Update, crop_avatars.run_if(in_state(AppScreen::Game)))
            // A stack sheet's keys: nothing else on the board reads an
            // arrow, so it needs no place in the chain.
            .add_systems(Update, scroll_stack.run_if(in_state(AppScreen::Game)))
            // Likewise its own line: it reads what the chain wrote and
            // orders against none of it, and a menu placed a frame late
            // is a menu that was on the window the whole time.
            .add_systems(Update, place_menu.run_if(in_state(AppScreen::Game)));
    }
}

/// What a button on the board means.
#[derive(Component, Debug, Clone, PartialEq)]
pub enum Click {
    Target(Target),
    /// An entry of the action map.
    Entry(usize),
    /// A route through the encountered ICE (`Game::breaks`).
    Break(usize),
    /// Take the last move back (`Game::back_label`).
    TakeBack,
    /// Pass the rest of the run, or stop (`Game::run_pass_label`).
    PassTheRun,
    /// Answer the optional trigger on the prompt this way every time
    /// (`Game::optional_prompt`).
    Remember(Answer),
    /// A control-bar button.
    Control(Control),
    /// A face in a zone sheet: read the card over the sheet.
    Inspect(CardId),
    /// A column's "+N" strip: the server's stack sheet.
    Stack(ServerId),
    /// A row of a list sheet (the score area): open or close its details.
    Expand(usize),
    /// The gear.
    Options,
    /// Save the match so far as a file that replays (`save_report`).
    SaveReport,
    /// Open the report just saved on the replay board.
    WatchReport,
    CloseOverlay,
    /// The phase panel: open the timing chart (`board::timing`).
    Timing,
    ConfirmQuit,
    CancelQuit,
    /// Escape's button: the quit prompt.
    Quit,
    PlayAgain,
    Menu,
    Back,
    /// A lesson's opening words are read (`Intent::BeginLesson`).
    BeginLesson,
    /// A lesson's escape hatch (`Intent::EveryAction`).
    EveryAction,
    /// The lesson after this one in its track, on a fresh board.
    NextLesson,
    /// This lesson again from the start, after its match ended first.
    RetryLesson,
    /// A track's last lesson done: that side's starter game.
    StarterGame,
}

/// The match to put on the board once this one is gone: "Next lesson" and
/// "Try again" go from the board to the board, and leaving the screen
/// drops `ActiveMatch`, so the next one waits here until `leave` has.
#[derive(Resource)]
pub struct NextMatch(pub ActiveMatch);

/// The board, respawned when the view moves.
#[derive(Component)]
pub struct Board;

/// A server's column, for a test that reads what a column holds.
#[derive(Component)]
pub struct ServerColumn(pub ServerId);

/// A stack sheet's scrolling list, for the keys that scroll it.
#[derive(Component)]
pub struct StackScroll;

/// A row of a stack sheet: the card it is about, for a test that reads
/// the order.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackRow(pub InstallId);

/// A column's "+N" strip: the cards of the server it has no strip for.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoreStrip(pub ServerId);

/// A server's nameplate, on the Corp's edge of its column: its name and
/// count in a frame one strip tall.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerPlate(pub ServerId);

/// The frame on a nameplate — a server's or a rig row's — and the key it
/// was drawn from.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlateFrame(pub &'static str);

/// The picture behind a tile's words, and the key it was drawn from.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TileArt(pub &'static str);

/// A token's badge — on a tile or under a rig card — and the key of the
/// glyph it shows.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TokenBadge(pub &'static str);
/// Where the pointer was last seen, in logical window pixels: read off
/// `CursorMoved`, because the window's own `cursor_position` needs a
/// window and the headless tests have none.
#[derive(Resource, Default)]
pub struct Pointer(pub (f32, f32));

/// A place a dragged card may be dropped: the node's own box is the
/// target, so the drop is hit-tested against what is laid out rather than
/// against whatever the focus system last marked hovered.
///
/// **Most places are one target; an area can be two.** The rig is where a
/// Runner install goes and is also part of the table an event is played
/// on, so its box is both, in that order; the first the held card is lit
/// for is what the drop means. Two nested nodes would have said the same
/// and cost the rig a layer it has no height for.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct DropPlace(pub Vec<Target>);

impl DropPlace {
    pub fn one(target: Target) -> Self {
        DropPlace(vec![target])
    }

    /// The target of this place the held card is lit for, if any.
    pub fn lit_for(&self, lit: &[Target]) -> Option<&Target> {
        self.0.iter().find(|target| lit.contains(target))
    }
}

/// The count under a stack of identical rig cards, for a test to read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RigCopies(pub usize);

/// A Trojan on the tile of the ice that hosts it: a button of its own,
/// the Trojan's click and the Trojan's sheet
/// (`netrunner_client::board::rig::hosted_on`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedChip(pub InstallId);

/// A Trojan's copy in the program row, drawn faintly because its home is
/// on its ice (`netrunner_client::board::rig::is_ghost`). It is still the
/// Trojan's button: a ghost is a second door to one menu, never a picture.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ghost(pub InstallId);

/// The wash over a ghost's text face. A scan replaces the face's
/// children when it lands (`card_images`), taking the wash with it, and
/// is faded by `fade_ghosts` instead.
#[derive(Component)]
struct GhostWash;

/// How much of a ghost shows through: the scan's alpha, and one less the
/// wash's over a text face.
const GHOST_ALPHA: f32 = 0.4;

/// A card's place in the person's own hand, for a drag to read.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandSlot(pub usize);

/// The phase panel in the right column, and one step chip on it, for a
/// test to read. Hidden rather than despawned when the setting is off.
#[derive(Component)]
pub struct PhaseBarRow;

/// The right column's panel that shows the Runner's identity while a run
/// is on, or the ICE while the run encounters one; empty, and taking no
/// room, when no run is on.
#[derive(Component)]
pub struct RunIdentity;

/// The Runner's name in the run panel, for a test to read.
#[derive(Component)]
pub struct RunIdentityName;

/// The cropped scan in the run panel, when one is cached, and the copy
/// it was drawn from: a smaller copy stands in until the panel's own
/// width has decoded, and the panel is redrawn when it has.
#[derive(Component)]
pub struct RunIdentityArt(Handle<Image>);

/// The right column's head (the status line, Quit and the gear), whose
/// laid-out height the encounter panel's art is sized against.
#[derive(Component)]
pub struct RailHeader;

/// The encountered ICE's name in the run panel, for a test to read.
#[derive(Component)]
pub struct EncounterName;

/// One of the encounter panel's lines under the name — the type line,
/// the strength, a marked subroutine — for a test to read.
#[derive(Component)]
pub struct EncounterLine;

#[derive(Component)]
pub struct PhaseStep(pub netrunner_client::board::phase::State);

/// A row of the list of keys, for a test to count.
#[derive(Component)]
pub struct HelpRow;

/// A step of the timing chart, lit when the game is at it, with the rule
/// it stands for — for a test to read.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TimingStep {
    pub cr: &'static str,
    pub lit: bool,
}

/// One line of an install's state on its sheet, for a test to read.
#[derive(Component)]
pub struct InstallFact;
/// A side's HUD — its avatar bar, the numbers either side of the
/// avatar — for a test that reads where its numbers are.
#[derive(Component)]
pub struct HudPanel(pub Side);
/// A side's avatar: its identity's art in a disc on the bar, and the
/// identity's click. For a test to press, and to read which it is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Avatar(pub Side);
/// A side's avatar bar, and whether it is lit for that side's turn.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarBar {
    pub side: Side,
    pub lit: bool,
}

/// The identities' art, cropped for the avatars: the scan and the square
/// of it the disc shows, by identity. Kept across redraws, because the
/// crop needs the decoded picture's size and the board is redrawn
/// without the image assets in hand; the board draws from it the frame
/// it is filled, so a redraw never shows the stand-in for a frame first.
#[derive(Resource, Default)]
pub struct AvatarCrops(std::collections::HashMap<CardId, (Handle<Image>, Rect)>);

/// The width the avatar's scan is decoded at: its crop is
/// `layout::AVATAR_ART`'s side of it, enough for the largest disc at a
/// display scale of 1.5.
const AVATAR_SCAN: FaceSize = FaceSize::Board(180);
/// One number on a HUD, as `hud::readouts` gave it, for a test to read.
#[derive(Component, Debug, Clone)]
pub struct HudReadout {
    pub label: &'static str,
    pub value: String,
}
/// A row of the score-area sheet, by position, for a test to press.
#[derive(Component)]
pub struct ScoreRow(pub usize);
/// The open row's details, by position, for a test to find.
#[derive(Component)]
pub struct ScoreDetails(pub usize);

/// The pacer the match's messages go through.
#[derive(Resource)]
pub struct Pace(pub Pacer);
/// The prompt, its decisions and the flat panel, respawned when the view
/// moves or a submit closes it.
#[derive(Component)]
pub struct Rail;
/// The control bar, respawned with the rail: it is a function of the
/// action map.
#[derive(Component)]
pub struct ControlBar;
/// The action panel's scroll column, inside the rail.
#[derive(Component)]
struct ActionList;
#[derive(Component)]
struct LogList;
#[derive(Component)]
struct LogScroll;
/// The log and its scrollbar; shown or hidden by the play history
/// preference, never despawned.
#[derive(Component)]
pub struct LogRow;
/// A card's name in the match log: a span of the line's text that opens
/// the card (Phase 7 §8 item 17), read off `actions::LogLine`.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct LogName(pub CardId);
/// The full-window overlay, when one is up.
#[derive(Component)]
pub struct Overlay;
/// The end-of-match table, under the result.
#[derive(Component)]
pub struct EndTable;
/// The start-of-game box's row of identities, over the opening hand.
#[derive(Component)]
pub struct OpeningIdentities;
/// The decision the game is waiting on — the mulligan, an access, a
/// trace bid, a choice a card asks — as a pop-up in the middle of the
/// screen, where the eyes are, rather than buttons on the rail. It sits
/// over the board and does not block it: a card can still be read, and
/// a sheet opens above it. Respawned with the rail.
#[derive(Component)]
pub struct DecisionPopup;
/// A card the decision pop-up draws — a selection's candidate, the card an
/// install is placing, the card asking or the card accessed — which a
/// secondary click reads in the card sheet, as it would on the board.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ChoiceCard(pub CardId);
/// A click's menu of a target's actions, above its card.
/// Respawned with the rail, like the pop-up; over it and under the
/// overlays.
#[derive(Component)]
pub struct ActionsMenu;
/// The menu's panel and each of its buttons: a primary click that
/// presses none of these closes the menu.
#[derive(Component)]
struct MenuPart;
#[derive(Component)]
pub struct StatusLine;

#[derive(Resource)]
pub struct Model(pub Game);

#[derive(Resource, Default)]
pub(crate) struct Dirty {
    board: bool,
    rail: bool,
    log: bool,
    overlay: bool,
    /// A beat of the trail, with the board still: the run panel follows
    /// it.
    trail: bool,
    /// The right column's status line, phase panel and run panel, which
    /// follow the board and the trail and also a setting.
    side: bool,
}

impl Dirty {
    pub(crate) fn all(&mut self) {
        self.board = true;
        self.rail = true;
        self.log = true;
        self.overlay = true;
        self.trail = true;
        self.side = true;
    }
}

/// Intents raised by a key, applied by `controls` with the pressed ones
/// so every outcome is handled in one place.
#[derive(Resource, Default)]
pub(crate) struct Pending(Vec<Intent>);

/// The card width the board is drawn at, and the window it was computed
/// for. `fit` keeps it current; a change redraws the board.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct BoardFit {
    /// The person's own face width; the opponent's side is drawn at
    /// `layout::OPPONENT_SCALE` of it.
    pub face: f32,
    pub window: Vec2,
    /// The chair the width was computed for.
    pub chair: Side,
    /// What the window has over the fixed rows at this width
    /// (`layout::spare_height`): the rig's rows grow into it.
    pub spare: f32,
    /// How many strips a server column shows (`layout::server_slots`).
    pub slots: usize,
}

impl Default for BoardFit {
    fn default() -> Self {
        // The headless tests' window: no `Window` exists there.
        Self { face: 0.0, window: Vec2::new(1280.0, 800.0), chair: Side::Runner, spare: 0.0, slots: layout::server_slots(800.0) }
    }
}

impl BoardFit {
    /// The face width a side's cards are drawn at from this chair.
    fn area_face(&self, side: Side) -> f32 {
        layout::area_face(side, self.chair, self.face)
    }

    /// A side's cards, at the size their side of the table is drawn.
    pub fn size_of(&self, side: Side) -> FaceSize {
        FaceSize::Board(self.area_face(side).round() as u16)
    }

    fn board_width(&self) -> f32 {
        layout::board_width(self.window.x)
    }
}

/// What the view puts on the board, for the fit.
fn counts(game: &Game) -> Counts {
    let human_is_runner = game.side == Side::Runner;
    // The three centrals are always drawn.
    let remotes = game.view.as_ref().map_or(0, |view| view.corp.servers.iter().filter(|s| matches!(s.server, ServerId::Remote(_))).count());
    Counts { servers: 3 + remotes, human_is_runner }
}

/// Loads the board's pictures (`board_art`) for the skin in use and the
/// Corp's faction, and again when either changes, redrawing the board so
/// a new skin's buildings replace the old ones without leaving the
/// screen. The faction is the Corp identity's, which is public to both
/// chairs (`CorpClientView::identity`), so it is known from the first
/// view and fixed for the match. Loading decodes files, so it happens on
/// a change and never per frame. Nothing without `Assets<Image>`, which
/// is the headless tests: a plate there is its label alone.
#[allow(clippy::too_many_arguments)]
fn board_pictures(
    mut commands: Commands,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    model: Option<Res<Model>>,
    skin: Res<crate::skin::Skin>,
    art: Option<Res<BoardArt>>,
    images: Option<ResMut<Assets<Image>>>,
    mut dirty: ResMut<Dirty>,
) {
    let Some(mut images) = images else { return };
    let faction = model.as_ref().and_then(|model| model.0.view.as_ref()).and_then(|view| view.corp.identity.as_ref()).and_then(|id| core.registry.get(id)).and_then(|card| card.faction);
    let style = board_art::Style { skin: skin.folder.clone(), faction, basic: core.settings.desktop.basic_graphics };
    if art.is_some_and(|art| art.loaded_for == style) {
        return;
    }
    commands.insert_resource(BoardArt::load(style, &theme, &mut images));
    dirty.board = true;
}

/// Fills [`AvatarCrops`] for the two identities in play: asks for each
/// scan at [`AVATAR_SCAN`] once it is cached, and when it has decoded,
/// crops the square the disc shows (`layout::avatar_crop`) and redraws
/// the board. Once per identity per visit — a crop is kept — and nothing
/// without `Assets<Image>`, which is the headless tests: the disc there
/// is its faction's mark.
fn crop_avatars(
    model: Option<Res<Model>>,
    core: Res<ClientCore>,
    mut images: ResMut<CardImages>,
    assets: Option<Res<Assets<Image>>>,
    mut crops: ResMut<AvatarCrops>,
    mut dirty: ResMut<Dirty>,
) {
    let (Some(model), Some(assets)) = (model, assets) else { return };
    let Some(view) = &model.0.view else { return };
    for id in [&view.corp.identity, &view.runner.identity].into_iter().flatten() {
        if crops.0.contains_key(id) {
            continue;
        }
        let Some(code) = core.registry.get(id).and_then(|card| card.numeric_id) else { continue };
        match images.face(code, AVATAR_SCAN) {
            Some(scan) => {
                if let Some(image) = assets.get(&scan) {
                    let size = image.size_f32();
                    let [x0, y0, x1, y1] = layout::avatar_crop((size.x, size.y));
                    crops.0.insert(id.clone(), (scan, Rect::new(x0, y0, x1, y1)));
                    dirty.board = true;
                }
            }
            // Asked for only when the view moves, not every frame: the
            // store's status reads the disk, and a scan the person has not
            // downloaded is not going to appear between two frames.
            None if model.is_changed() => {
                if let netrunner_card_sync::ImageStatus::Cached(path) = core.images.status(code) {
                    images.request(code, AVATAR_SCAN, path);
                }
            }
            None => {}
        }
    }
}

/// Recomputes the face width from the window and the view, and marks the
/// board for a redraw when it moved. Runs every frame and is cheap: a
/// handful of comparisons.
pub(crate) fn fit(windows: Query<&Window, With<PrimaryWindow>>, model: Option<Res<Model>>, fit: Option<ResMut<BoardFit>>, mut dirty: ResMut<Dirty>) {
    let (Some(model), Some(mut fit)) = (model, fit) else { return };
    let window = windows.single().map_or(fit.window, |w| Vec2::new(w.width(), w.height()));
    let counts = counts(&model.0);
    let face = layout::face_width((window.x, window.y), counts);
    if (face - fit.face).abs() > 0.5 || window != fit.window || model.0.side != fit.chair {
        fit.face = face;
        fit.window = window;
        fit.chair = model.0.side;
        fit.spare = layout::spare_height(window.y, face, counts);
        fit.slots = layout::server_slots(window.y);
        dirty.board = true;
        // The pop-up is sized from the window too — capped at it, with
        // the cards drawn in what its words leave — so a resize has to
        // rebuild the rail's floating panels and not only the board.
        // (`place_menu` still earns its place: it re-anchors the menu
        // every frame the card's own box moves under it, which no resize
        // is involved in.)
        dirty.rail = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    mut commands: Commands,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    active: Option<Res<ActiveMatch>>,
    replay: Option<Res<ActiveReplay>>,
    mut images: Option<ResMut<Assets<Image>>>,
    dev: Option<Res<crate::dev::Dev>>,
    mut last_table: ResMut<table::LastTable>,
) {
    commands.init_resource::<Dirty>();
    // A match being played, or a recorded one being stepped through
    // (`screens::replay`): the same board either way.
    let source = match (&active, &replay) {
        (Some(active), _) => {
            let game = match &active.lesson {
                Some(lesson) => {
                    let board = LessonBoard { has_next: crate::screens::learn::next_after(&lesson.id).is_some(), ..LessonBoard::new(lesson.title.clone(), lesson.intro.clone()) };
                    Game::lesson(core.registry.clone(), active.handle.side(), board)
                }
                None => Game::new(core.registry.clone(), active.handle.side()),
            };
            Some((game, active.handle.side()))
        }
        (None, Some(replay)) => Some((crate::screens::replay::board_for(&core, &replay.0), replay.0.side())),
        (None, None) => None,
    };
    let Some((game, side)) = source else {
        commands.spawn((screen_root(AppScreen::Game, &theme), children![
            widgets::heading(&theme, AppScreen::Game.title()),
            widgets::dim(&theme, "No game in progress. Start one from Play vs Computer."),
            widgets::button(&theme, "Back", Val::Auto, Click::Back),
        ]));
        return;
    };
    let replaying = game.replay.is_some();
    let mut pacer = Pacer::new(side, core.settings.desktop.animation_speed);
    pacer.hold_at_encounter = dev.is_some_and(|dev| dev.hold_run);
    commands.insert_resource(Pace(pacer));

    // No scroll area, ever: the board's rows are sized by `BoardFit` to
    // fit, and `Overflow::clip` is the backstop, not the design.
    let board = commands
        .spawn((
            Board,
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                min_height: px(0),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(layout::ROW_GAP),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .id();
    let rail = commands
        .spawn((Rail, Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }))
        .id();
    let log_scroll = commands
        .spawn((
            LogScroll,
            bevy::ui_widgets::ScrollArea,
            BackgroundColor(theme.panel),
            Node { width: percent(100), height: px(200), flex_shrink: 0.0, flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), padding: UiRect::all(px(6)), ..default() },
        ))
        .with_children(|parent| {
            parent.spawn((LogList, Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }));
        })
        .id();
    let log_row = commands
        .spawn((LogRow, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, column_gap: px(4), height: px(200), ..default() }))
        .add_child(log_scroll)
        .with_children(|parent| {
            parent.spawn(widgets::scrollbar(&theme, log_scroll));
        })
        .id();
    // The right column's head: where the match is, Quit and the gear.
    // It was a bar across the top of the window, which cost every card
    // on the board its height to say what the column beside it says.
    let header = commands
        .spawn((RailHeader, Node { width: percent(100), min_height: px(layout::TOP_BAR), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8), ..default() },))
        .with_children(|parent| {
            parent.spawn((StatusLine, widgets::dim(&theme, "Setting up…"), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }));
            // A replay has nothing to lose, so its way out says where it
            // goes rather than asking.
            parent.spawn(widgets::button(&theme, if replaying { "Close" } else { "Quit" }, Val::Auto, Click::Quit));
            // The gear in the corner, where a person looks for options.
            parent.spawn(widgets::gear_button(&theme, images.as_deref_mut(), Click::Options));
        })
        .id();
    let phase_panel = commands
        .spawn((PhaseBarRow, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, ..default() }))
        .id();
    let run_panel = commands
        .spawn((RunIdentity, Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, ..default() }))
        .id();
    // The one column on the right: the header, the Runner on a run, the
    // prompt, the log and the phase. It carries the vertical padding the
    // root no longer does, so its buttons keep off the window's edges.
    // The phase panel is last because its height moves with the phase —
    // a label wraps, the window's line comes and goes — and under the
    // header it moved the Runner's art and the encountered ICE with it;
    // at the foot of the column the prompt's room is what gives.
    let rail_column = commands
        .spawn((Node {
            width: px(layout::RAIL_WIDTH),
            height: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            padding: UiRect::vertical(px(layout::PADDING)),
            ..default()
        },))
        .add_child(header)
        .add_child(run_panel)
        .add_child(rail)
        .add_child(log_row)
        .add_child(phase_panel)
        .id();
    let body = commands
        .spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(layout::BODY_GAP), ..default() },))
        .add_child(board)
        .add_child(rail_column)
        .id();
    // The field, chosen once for the match rather than per redraw, and
    // spawned before everything else so every later sibling paints over
    // it. Only where `Assets<Image>` exists: the headless tests build the
    // client without an asset plugin, and the board there has no field,
    // which costs them nothing since nothing they assert is a picture.
    let backdrops: Vec<Entity> = match images.as_deref_mut() {
        Some(images) => {
            let installed = table::available();
            let last = last_table.0.take();
            let folder = table::resolve(&core.settings.desktop.table, &installed, table_nonce(), last.as_deref(), core.settings.desktop.basic_graphics);
            last_table.0 = folder.clone().or(last);
            // A named table whose files have gone falls back to the
            // painted ground, which is the tier that always works.
            let field = folder.as_deref().and_then(table::base).unwrap_or_else(|| table::paint(&theme));
            let mut spawned = vec![commands.spawn(table::backdrop(images.add(field))).id()];
            if let Some(overlay) = folder.as_deref().and_then(table::overlay) {
                spawned.push(commands.spawn(table::backdrop(images.add(overlay))).id());
            }
            spawned
        }
        None => Vec::new(),
    };
    let mut root = commands.spawn(screen_root(AppScreen::Game, &theme));
    root.entry::<Node>().and_modify(|mut node| {
        node.align_items = AlignItems::Stretch;
        // Sides only: the hands sit on the window's top and bottom edges.
        node.padding = UiRect::horizontal(px(layout::PADDING));
        node.row_gap = px(0);
        node.overflow = Overflow::clip();
    });
    for backdrop in backdrops {
        root.add_child(backdrop);
    }
    root.add_child(body);
    commands.insert_resource(BoardFit::default());
    let mut game = game;
    game.answers = core.settings.answers.clone();
    commands.insert_resource(Model(game));
    let mut dirty = Dirty::default();
    dirty.all();
    commands.insert_resource(dirty);
}

/// Marks the bands the table guide draws, so the next frame's are not
/// drawn on top of this frame's.
#[derive(Component)]
struct TableGuide;

/// `NETRUNNER_TABLE_GUIDE=1`: a band over each row of the board, with a
/// centre line, so a field can be painted to where the cards actually
/// fall.
///
/// The person authoring a table has to put its horizon and its vanishing
/// point somewhere, and "somewhere" is only useful if it agrees with the
/// layout. The bands are read from the **laid-out boxes** rather than
/// recomputed from `models::layout`, so what they show is where the
/// cards are, including every overlap and clamp the fit applied.
///
/// Rebuilt every frame while it is on, because `redraw` despawns the
/// board's children wholesale and a guide that survived that would be
/// describing the previous layout. It is a dev hook; the cost is a dev's.
fn table_guide(
    mut commands: Commands,
    theme: Res<Theme>,
    dev: Option<Res<crate::dev::Dev>>,
    board: Query<(Entity, &Children, &ComputedNode, &UiGlobalTransform), With<Board>>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    drawn: Query<Entity, With<TableGuide>>,
) {
    if !dev.is_some_and(|dev| dev.table_guide) {
        return;
    }
    // The bands hang off one container rather than off the board
    // directly, and the loop below skips it. Both halves matter: the
    // first cut parented each band to the board, so the next frame read
    // the board's children — bands included — and drew a band for every
    // band. It grew by a row a frame (6, 12, 18 …) and looked like a
    // despawn that was not working.
    for entity in &drawn {
        commands.entity(entity).despawn();
    }
    let Ok((board_entity, children, board_node, board_at)) = board.single() else { return };
    let board_box = anchor_of(board_node, board_at);
    let container = commands
        .spawn((
            TableGuide,
            GlobalZIndex(5),
            Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
        ))
        .id();
    commands.entity(board_entity).add_child(container);
    for (index, child) in children.iter().filter(|child| !drawn.contains(*child)).enumerate() {
        let Ok((node, at)) = boxes.get(child) else { continue };
        let row = anchor_of(node, at);
        // The child's box in the board's own coordinates, since the band
        // sits in a container that fills the board.
        let left = row.x - row.width / 2.0 - (board_box.x - board_box.width / 2.0);
        let top = row.y - row.height / 2.0 - (board_box.y - board_box.height / 2.0);
        let tint = if index % 2 == 0 { theme.accent } else { theme.danger };
        // Outlined, never filled: the whole point is to see the field
        // under the rows, and a wash over it would hide what is being
        // painted to.
        commands.entity(container).with_children(|guide| {
            guide.spawn((
                BackgroundColor(Color::NONE),
                BorderColor::all(tint),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(left),
                    top: px(top),
                    width: px(row.width),
                    height: px(row.height),
                    border: UiRect::all(px(1)),
                    padding: UiRect::all(px(2)),
                    ..default()
                },
                // `widgets::dim` already carries a `TextColor`; a second
                // one in the same bundle is the duplicate-component panic.
                children![(
                    Text::new(format!("row {index} · y {:.0}…{:.0} · h {:.0}", top, top + row.height, row.height)),
                    theme.font(size::SMALL),
                    TextColor(tint),
                )],
            ));
        });
    }
}

/// What decides a random table. The wall clock, because the choice is
/// cosmetic and per match: seeding it from the game's own seed would tie
/// the field to the deal, so replaying a seed to look at a bug would
/// change the picture with it.
fn table_nonce() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|since| since.as_nanos() as u64).unwrap_or(0)
}

/// Saves the match so far where the client keeps its reports, and says
/// where in words a person can act on: the file, and the command that
/// opens it at the moment it was saved. A failure is a sentence too,
/// never a panic — a report that cannot be written must not end the game.
fn save_report(core: &ClientCore, handle: &netrunner_client::play::MatchHandle) -> (String, Option<std::path::PathBuf>) {
    let Some(dir) = &core.reports_dir else {
        return (format!("No bug report saved: there is no data directory; set {}", netrunner_client::bug_report::REPORTS_DIR_ENV), None);
    };
    let (header, history) = handle.record();
    match netrunner_client::bug_report::save(dir, &header, &history) {
        Ok(path) => (format!("Bug report saved to {} — it is under Replays, or open it with `netrunner_cli replay {}`", path.display(), path.display()), Some(path)),
        Err(error) => (format!("No bug report saved: {error}"), None),
    }
}

/// Remembers a report just saved on the board, for its "Watch it".
fn saved(model: &mut Game, notices: &mut Notices, (line, path): (String, Option<std::path::PathBuf>)) {
    notices.push(line.clone());
    model.saved_report = Some(line);
    model.saved_report_path = path;
}

/// Leaving the screen ends the match: dropping the handle quits it, and
/// the form gets the choice to reopen on.
fn leave(world: &mut World) {
    world.remove_resource::<Model>();
    world.remove_resource::<Pace>();
    world.remove_resource::<ActiveReplay>();
    if let Some(active) = world.remove_resource::<ActiveMatch>()
        && let Some(choice) = active.choice
    {
        world.insert_resource(LastGame(choice));
    }
    if let Some(NextMatch(next)) = world.remove_resource::<NextMatch>() {
        world.insert_resource(next);
    }
}

/// Drains the match's messages into the pacer, and the beats due now
/// into the model: a run's events move the trail and the run panel, a
/// message moves the board.
///
/// **A stall saves a bug report by itself**, the moment it arrives: it is
/// the report that matters most, the person may not think to press
/// anything, and the match thread has already stopped, so the record is
/// the whole game.
#[allow(clippy::too_many_arguments)]
fn poll(
    active: Option<ResMut<ActiveMatch>>,
    model: Option<ResMut<Model>>,
    pace: Option<ResMut<Pace>>,
    time: Res<Time>,
    mut dirty: ResMut<Dirty>,
    dev: Option<ResMut<crate::dev::Dev>>,
    mut core: ResMut<ClientCore>,
    mut notices: ResMut<Notices>,
) {
    // A replay has no match: its steps are pushed into the pacer by
    // `screens::replay`, and the beats are released here the same way.
    let (Some(mut model), Some(mut pace)) = (model, pace) else { return };
    let mut active = active;
    while let Some(message) = active.as_mut().and_then(|active| active.handle.poll()) {
        if let Some(active) = &active
            && matches!(message, netrunner_client::play::MatchMessage::Stalled { .. })
        {
            saved(&mut model.0, &mut notices, save_report(&core, &active.handle));
        }
        // A lesson is done the moment its last step is, whatever the
        // person does with the closing words: ticked in the settings
        // file both clients read.
        if let Some(lesson) = active.as_ref().and_then(|active| active.lesson.as_ref())
            && matches!(message, netrunner_client::play::MatchMessage::LessonComplete { .. })
            && core.settings.lessons_done.insert(lesson.id.clone())
            && let Err(error) = core.save_settings()
        {
            notices.push(format!("Settings not saved: {error}"));
        }
        pace.0.push(message);
    }
    for beat in pace.0.tick(time.elapsed()) {
        match beat {
            Beat::Steps(events) => {
                if model.0.apply(Intent::RunStep(events)) == Outcome::Redraw {
                    dirty.trail = true;
                }
            }
            Beat::Apply(message) => {
                // The one message outcome that acts: a lone pass taken
                // for the person. `submit` fails only once the match has
                // ended, and its `Ended` is already on the way.
                if let Outcome::Submit(action) = model.0.apply(Intent::Message(MatchMessageRef(message)))
                    && let Some(active) = &active
                {
                    let _ = active.handle.submit(action);
                }
                dirty.all();
            }
        }
    }
    // Held at an encounter for a screenshot: the autoplay is done, so
    // the shot is taken with the run in flight.
    if pace.0.held && let Some(mut dev) = dev {
        dev.autoplayed = dev.autoplayed.max(dev.autoplay);
    }
}

/// The dev hook's hand: while `NETRUNNER_AUTOPLAY` has decisions left,
/// takes one from the panel each time the board is awaiting — the
/// `applied`th entry, so the choice wanders through the list and the
/// game develops rather than clicking for credits forever. Off, and
/// ignored, for a person.
fn autoplay(
    dev: Option<ResMut<crate::dev::Dev>>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
    nodes: Query<(&Click, &ComputedNode, &UiGlobalTransform)>,
    client: Res<ClientCore>,
) {
    let (Some(mut dev), Some(model)) = (dev, model) else { return };
    // A match that has stopped has no decisions left to take, so the
    // autoplay is done: the screenshot is taken of how it stopped and the
    // client exits. It used to wait for a count a finished game could
    // never reach, and a stall left the window open and hung for good —
    // what the person watching a Mutual Favor livelock saw (§4ag).
    if dev.autoplayed < dev.autoplay && (model.0.stalled.is_some() || model.0.over.is_some()) {
        match &model.0.stalled {
            Some(reason) => warn!("dev: the match stopped after {} of {} autoplayed decisions: {reason}", dev.autoplayed, dev.autoplay),
            None => info!("dev: the match ended after {} of {} autoplayed decisions", dev.autoplayed, dev.autoplay),
        }
        dev.autoplayed = dev.autoplay;
        return;
    }
    // A lesson's words, put away before anything is played.
    if dev.begin && model.0.intro_open() {
        dev.begin = false;
        pending.0.push(Intent::BeginLesson);
        return;
    }
    // Nothing is played under the words, by a hand or by this.
    if model.0.intro_open() {
        return;
    }
    if dev.options && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        // The gear, pressed once the board has settled.
        dev.options = false;
        pending.0.push(Intent::ToggleOptions);
        return;
    }
    // The first decision at which a card in hand has somewhere to go: a
    // card held anywhere else lights nothing, and waiting for the
    // person's own action phase can wait for ever — the autoplay may have
    // stopped at a prompt inside the opponent's turn.
    if dev.drag && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        let held = model.0.hand.cards().iter().position(|card| !model.0.actions.destinations_for_hand_card(card).is_empty());
        match held {
            Some(slot) => {
                dev.drag = false;
                pending.0.push(Intent::DragPress { slot, at: (0.0, 0.0) });
                // Up the board from the press, so the shot shows the card
                // carried over the table rather than off its edge.
                pending.0.push(Intent::DragMove { at: (160.0, -520.0) });
            }
            // Nothing in hand has anywhere to go at this decision (a hand
            // of operations, or a prompt mid-turn): take one more action
            // and look again, rather than holding the shot for ever.
            None => dev.autoplay += 1,
        }
    }
    if dev.drag && model.0.finished() {
        dev.drag = false;
    }
    if dev.keys && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        dev.keys = false;
        pending.0.push(Intent::Shortcut(Shortcut::Help));
    }
    // Not waiting for the person's decision, as the list of keys does:
    // the chart is worth reading while the other side thinks, and a run
    // held for the shot is often at a window the bot holds.
    if dev.timing && model.0.view.is_some() && dev.autoplayed >= dev.autoplay {
        dev.timing = false;
        pending.0.push(Intent::ToggleTiming);
    }
    if let Some(pick) = dev.menu.filter(|_| model.0.awaiting && dev.autoplayed >= dev.autoplay) {
        dev.menu = None;
        // Every box the board laid out, with what a click on it would
        // mean and how many entries that is — which is all three picks
        // need, so the hook learns nothing new about the view.
        let boxes = || {
            nodes.iter().filter_map(|(click, node, transform)| match click {
                Click::Target(target) => Some((target.clone(), model.0.entries_for(target).len(), anchor_of(node, transform))),
                _ => None,
            })
        };
        let picked = match pick {
            crate::dev::MenuPick::Hand => {
                let hand = model.0.view.as_ref().and_then(|view| match model.0.side {
                    Side::Corp => view.corp.hq_cards.clone(),
                    Side::Runner => view.runner.grip_cards.clone(),
                });
                // The first card with an action, else the first card: a
                // menu with nothing in it is a look worth taking too.
                let hand = hand.unwrap_or_default();
                hand.iter().find(|card| !model.0.actions.for_hand_card(card).is_empty()).or(hand.first()).map(|card| {
                    let target = Target::HandCard(card.clone());
                    // The card's own box, as the click would have read it.
                    let over = boxes().find(|(candidate, _, _)| *candidate == target).map_or_else(Anchor::default, |(_, _, over)| over);
                    (target, over)
                })
            }
            crate::dev::MenuPick::Most => boxes().max_by_key(|(_, entries, _)| *entries).filter(|(_, entries, _)| *entries > 0).map(|(target, _, over)| (target, over)),
            crate::dev::MenuPick::Top => boxes()
                .filter(|(_, entries, _)| *entries > 0)
                .min_by(|(_, _, a), (_, _, b)| a.y.total_cmp(&b.y))
                .map(|(target, _, over)| (target, over)),
        };
        if let Some((target, over)) = picked {
            pending.0.push(Intent::Click { target, over });
        }
    }
    if dev.sheet && model.0.awaiting && dev.autoplayed >= dev.autoplay {
        dev.sheet = false;
        // The first Corp install on the board, ice before root, in the
        // engine's order: the tile's own sheet.
        let first = model.0.view.as_ref().and_then(|view| view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).map(|c| c.install_id).next());
        if let Some(id) = first {
            pending.0.push(Intent::Inspect(Target::Install(id)));
        }
    }
    if model.0.awaiting
        && dev.autoplayed >= dev.autoplay
        && let Some((target, card)) = dev.pile.take()
    {
        let first = model.0.view.as_ref().and_then(|view| match &target {
            Target::Server(ServerId::Archives) => view.corp.archives.iter().find_map(|c| c.card.clone()),
            Target::Server(ServerId::Hq) => view.corp.hq_cards.as_ref().and_then(|hand| hand.first().cloned()),
            Target::Pile(Pile::Heap) => view.runner.heap.first().cloned(),
            _ => None,
        });
        pending.0.push(Intent::Inspect(target));
        if card && first.is_some() {
            pending.0.push(Intent::InspectCard(first));
        }
    }
    if model.0.awaiting
        && dev.autoplayed >= dev.autoplay
        && let Some(side) = dev.agendas.take()
    {
        pending.0.push(Intent::Inspect(Target::Pile(Pile::Agendas(side))));
        pending.0.push(Intent::Expand(0));
    }
    if dev.autoplayed >= dev.autoplay || !model.0.awaiting || model.0.actions.is_empty() {
        return;
    }
    // Held at a card-selection prompt, a card's install, an access, or
    // an encounter with a way through, for a screenshot: the autoplay is done, and the pop-up is what is
    // shot. An access is not a `PendingDecision`, so it is asked of
    // `access::Access` rather than matched here.
    let held = model.0.view.as_ref().is_some_and(|view| match view.pending_decision {
        Some(PendingDecision::ChooseCards { .. }) => dev.hold_selection,
        Some(PendingDecision::ChooseServer { install: Some(_), .. }) => dev.hold_install,
        _ => dev.hold_access && Access::of(view, &client.registry).is_some(),
    }) || (dev.hold_break && !model.0.breaks.is_empty())
        || (dev.hold_ice && model.0.encounter().is_some())
        || (dev.hold_trojan && model.0.view.as_ref().is_some_and(|view| view.runner.rig.iter().any(netrunner_client::board::rig::is_ghost)))
        || dev.hold_stack.is_some_and(|n| model.0.view.as_ref().is_some_and(|view| view.corp.servers.iter().any(|s| s.ice.len() + s.root.len() >= n)))
        || (dev.hold_may && model.0.optional_prompt().is_some())
        || (dev.hold_run_pass.is_some() && model.0.run_pass_label().is_some());
    if held && dev.hold_run_pass == Some(true) {
        dev.hold_run_pass = None;
        pending.0.push(Intent::PassTheRun);
    }
    if held {
        dev.autoplayed = dev.autoplay;
        return;
    }
    dev.autoplayed += 1;
    let entries = &model.0.actions.entries;
    // At a card selection the wandering index toggled one card on and off
    // until the session's stall guard ended the game (Phase 5 §19 found
    // it, on Mutual Favor): a toggle leaves the same list behind, so
    // `applied` walked the same two entries forever. So a selection is
    // finished — confirmed once it can be, otherwise a card not yet picked
    // is added — and is still one of the listed entries.
    let selecting = model.0.view.as_ref().and_then(|view| match &view.pending_decision {
        Some(PendingDecision::ChooseCards { selected, .. }) => Some(selected.clone()),
        _ => None,
    });
    let index = selecting
        .and_then(|selected| {
            let confirm = entries.iter().position(|entry| entry.action == PlayerAction::ConfirmCardSelection);
            confirm.or_else(|| {
                entries.iter().position(|entry| matches!(&entry.action, PlayerAction::ToggleCardSelection { position } if !selected.contains(position)))
            })
        })
        // A Trojan hosted the moment one can be: still a listed entry,
        // taken ahead of the wandering pick.
        .or_else(|| dev.hold_trojan.then(|| entries.iter().position(|entry| matches!(entry.action, PlayerAction::InstallProgramOnIce { .. }))).flatten())
        // A deep remote: an install into the deepest one the Corp has,
        // else into any remote (a new one, before there is any).
        .or_else(|| {
            dev.hold_stack?;
            let view = model.0.view.as_ref()?;
            let deepest = view.corp.servers.iter().filter(|s| matches!(s.server, ServerId::Remote(_))).max_by_key(|s| s.ice.len() + s.root.len()).map(|s| s.server);
            let into = |wanted: Option<ServerId>| entries.iter().position(|entry| matches!(entry.action, PlayerAction::InstallCard { zone, .. } if matches!(zone, ServerId::Remote(_)) && wanted.is_none_or(|w| w == zone)));
            into(deepest).or_else(|| deepest.is_none().then(|| into(None)).flatten())
        })
        .unwrap_or(model.0.applied % entries.len());
    pending.0.push(Intent::Choose(index));
}

/// Escape is the board's: it closes what is open, and otherwise asks to
/// quit, so the navigation rule never leaves the game without asking.
fn escape(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<InputCaptured>, mut pending: ResMut<Pending>, model: Option<Res<Model>>) {
    if model.is_none() || !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    captured.0 = true;
    pending.0.push(Intent::Back);
}

/// Whether the key that turns the primary button into the secondary is
/// held: Ctrl, or Cmd — a Mac with one button reaches for either, and
/// neither means anything else on a board click.
fn secondary_modifier(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight])
}

/// One line of the match log: its words in the log's dim text, and each
/// name in it a span of its own, in the accent, that opens the card.
///
/// A span, not a button beside the words: a line is one `Text`, so it
/// wraps at word boundaries as it always did, and `bevy_ui`'s picking
/// hits a single span of it (`pick_ui_text_section`). A row of nodes, one
/// per word so the line could still wrap, was the alternative — about ten
/// nodes a line over eighty lines, rebuilt on every action. The indent a
/// narrated line carries in the terminal is dropped here, as it was.
fn spawn_log_line(parent: &mut ChildSpawnerCommands, theme: &Theme, line: &netrunner_client::actions::LogLine) {
    let spans = line.spans();
    let last = spans.len().saturating_sub(1);
    parent.spawn((widgets::dim(theme, ""), TextLayout::new(Justify::Left, LineBreak::WordBoundary))).with_children(|text| {
        for (index, (words, card)) in spans.into_iter().enumerate() {
            let words = if index == 0 { words.trim_start() } else { words };
            let words = if index == last { words.trim_end() } else { words };
            match card {
                Some(card) => {
                    text.spawn((TextSpan::new(words), theme.font(size::SMALL), TextColor(theme.accent), LogName(card.clone())));
                }
                None => {
                    let font = theme.font(size::SMALL);
                    match widgets::symbols::spans(theme, words, &font) {
                        Some(pieces) => {
                            for (words, face) in pieces {
                                text.spawn((TextSpan::new(words), face, TextColor(theme.text_dim)));
                            }
                        }
                        None => {
                            text.spawn((TextSpan::new(words), font, TextColor(theme.text_dim)));
                        }
                    }
                }
            }
        }
    });
}

/// A press on a name in the log opens that card to read, with either
/// button: a name has nothing to do but be read, so there is no menu for
/// the primary click to open (the §5 rule that a sheet carries no
/// actions). `Interaction` is set for nodes only and a span is not one,
/// so the press comes from picking, as an observer — which a headless
/// test without the picking plugins never triggers rather than failing
/// on a message nobody registered. Acted on at the span itself, not again
/// at each ancestor the press bubbles through.
fn open_a_logged_name(press: On<bevy::picking::events::Pointer<bevy::picking::events::Press>>, names: Query<&LogName>, mut pending: ResMut<Pending>) {
    if press.entity != press.original_event_target() {
        return;
    }
    if let Ok(LogName(card)) = names.get(press.entity) {
        pending.0.push(Intent::InspectCard(Some(card.clone())));
    }
}

/// The secondary click — the right button, or the primary with Ctrl or
/// Cmd held — on a hovered card or zone opens its sheet, to read. The
/// focus system sets `Interaction` for the primary button only and
/// never for the right, so the button is read from the input resource
/// and the target is whichever `Click::Target` node it left hovered (or
/// pressed, with the modifier). The plain primary press is `controls`'s,
/// which opens the menu. With a menu open, a primary click that presses
/// no part of it and no other card closes it; a press on a card is that
/// card's menu instead (or, on the same card, the menu closing), and
/// the menu's own button reaches `controls` as a `Pressed` and submits.
fn board_click(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    targets: Query<(&Interaction, &Click)>,
    choice_cards: Query<(&Interaction, &ChoiceCard)>,
    menu_parts: Query<&Interaction, With<MenuPart>>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
) {
    let Some(model) = model else { return };
    let modifier = secondary_modifier(&keys);
    let secondary = mouse.just_pressed(MouseButton::Right) || (modifier && mouse.just_pressed(MouseButton::Left));
    // A card in the decision pop-up is read like one on the board: in the
    // card sheet, over the pop-up, which a click away closes again.
    if secondary && let Some((_, card)) = choice_cards.iter().find(|(interaction, _)| matches!(interaction, Interaction::Hovered | Interaction::Pressed)) {
        pending.0.push(Intent::InspectCard(Some(card.0.clone())));
        return;
    }
    let hovered = targets.iter().find_map(|(interaction, click)| match (interaction, click) {
        (Interaction::Hovered | Interaction::Pressed, Click::Target(target)) => Some(target.clone()),
        _ => None,
    });
    let menu_open = model.0.menu.is_some();
    // A face in a sheet reads large on either button: a secondary click
    // is how a card is read everywhere else, and a sheet's face has no
    // menu for the primary one to open.
    let sheet_face = targets.iter().find_map(|(interaction, click)| match (interaction, click) {
        (Interaction::Hovered | Interaction::Pressed, Click::Inspect(card)) => Some(card.clone()),
        _ => None,
    });
    if secondary && let Some(card) = sheet_face {
        pending.0.push(Intent::InspectCard(Some(card)));
        return;
    }
    // A column's "+N" strip is the server's stack on either button.
    let stack = targets.iter().find_map(|(interaction, click)| match (interaction, click) {
        (Interaction::Hovered | Interaction::Pressed, Click::Stack(server)) => Some(*server),
        _ => None,
    });
    if secondary && let Some(server) = stack {
        pending.0.push(Intent::InspectStack(server));
        return;
    }
    if secondary {
        match hovered {
            Some(target) => pending.0.push(Intent::Inspect(target)),
            None if menu_open => pending.0.push(Intent::CloseMenu),
            None => {}
        }
        return;
    }
    if menu_open && mouse.just_pressed(MouseButton::Left) && hovered.is_none() && !menu_parts.iter().any(|i| *i == Interaction::Pressed) {
        pending.0.push(Intent::CloseMenu);
    }
}

/// A hand card's press, travel and release (`models::drag`). The pointer
/// comes from `CursorMoved` rather than the window, so a test with no
/// window can drive a drag; the press itself is `Interaction::Pressed` on
/// a face carrying a `HandSlot`, as the click is. A release with no travel
/// is handed back as the card's click, so the menu still opens from the
/// same press — `controls` never sees a hand card at all.
/// Whether a point is inside a laid-out box.
fn within(anchor: Anchor, (x, y): (f32, f32)) -> bool {
    (anchor.x - x).abs() <= anchor.width / 2.0 && (anchor.y - y).abs() <= anchor.height / 2.0
}

#[allow(clippy::too_many_arguments)]
fn drag_hand(
    mut moved: MessageReader<CursorMoved>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Query<(&HandSlot, &Interaction, &ComputedNode, &UiGlobalTransform)>,
    places: Query<(&DropPlace, &ComputedNode, &UiGlobalTransform)>,
    model: Option<Res<Model>>,
    mut pointer: ResMut<Pointer>,
    mut pending: ResMut<Pending>,
) {
    let Some(model) = model else {
        moved.clear();
        return;
    };
    for message in moved.read() {
        pointer.0 = (message.position.x, message.position.y);
    }
    let held = model.0.dragging.is_some();
    // Ctrl or Cmd with the primary button is the secondary click, which
    // reads the card; it never picks one up.
    if !held && mouse.just_pressed(MouseButton::Left) && !secondary_modifier(&keys) && !model.0.covered() {
        if let Some((slot, _, _, _)) = slots.iter().find(|(_, interaction, _, _)| matches!(interaction, Interaction::Pressed | Interaction::Hovered)) {
            pending.0.push(Intent::DragPress { slot: slot.0, at: pointer.0 });
        }
        return;
    }
    if held && mouse.pressed(MouseButton::Left) {
        pending.0.push(Intent::DragMove { at: pointer.0 });
    }
    if held && mouse.just_released(MouseButton::Left) {
        // The row as it is laid out: each face's centre, in the person's
        // own order, which is the order the slots were drawn in.
        let mut row: Vec<(usize, f32, Anchor)> = slots
            .iter()
            .map(|(slot, _, node, transform)| {
                let anchor = anchor_of(node, transform);
                (slot.0, anchor.x, anchor)
            })
            .collect();
        row.sort_by_key(|(slot, _, _)| *slot);
        let dragged = model.0.dragged_slot().or_else(|| model.0.dragging.as_ref().map(|drag| drag.what));
        let over = dragged.and_then(|slot| row.iter().find(|(at, _, _)| *at == slot)).map(|(_, _, anchor)| *anchor).unwrap_or_default();
        // A place the board lit for this card, under the pointer: the
        // innermost wins, so an ice tile takes the drop rather than the
        // column it sits in. Hit-tested against the laid-out boxes, since
        // the focus system does not follow a pointer with a button down.
        let lit = model.0.drop_places();
        let dropped_on = places
            .iter()
            .filter_map(|(place, node, transform)| Some((place.lit_for(&lit)?, anchor_of(node, transform))))
            .filter(|(_, anchor)| within(*anchor, pointer.0))
            .min_by(|a, b| (a.1.width * a.1.height).total_cmp(&(b.1.width * b.1.height)))
            .map(|(target, anchor)| (target.clone(), anchor));
        match dropped_on {
            Some((target, anchor)) => pending.0.push(Intent::DragDrop { target, over: anchor }),
            None => pending.0.push(Intent::DragRelease { over, slots: row.iter().map(|(_, x, _)| *x).collect() }),
        }
    }
}

/// The board's keys (`models::shortcuts`), read off the keyboard messages by
/// what they type, so a letter is found on any layout; a key held with
/// Ctrl, Cmd or Alt is left to the system. The model answers most of them.
/// Two need the screen: the pointer's keys, which act on whichever card or
/// zone is hovered as its click would, and the play helper, which is a
/// setting saved to the file as the options menu saves it.
#[allow(clippy::too_many_arguments)]
fn shortcuts(
    mut keyboard: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    targets: Query<(Entity, &Interaction, &Click)>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    model: Option<Res<Model>>,
    mut pending: ResMut<Pending>,
    mut core: ResMut<ClientCore>,
    mut notices: ResMut<Notices>,
    mut dirty: ResMut<Dirty>,
) {
    let Some(model) = model else {
        keyboard.clear();
        return;
    };
    if held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight, KeyCode::AltLeft, KeyCode::AltRight]) {
        keyboard.clear();
        return;
    }
    let shift = held.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for input in keyboard.read() {
        if input.state != ButtonState::Pressed || input.repeat {
            continue;
        }
        let key = match &input.logical_key {
            BevyKey::Space => shortcuts::Key::Space,
            BevyKey::Enter => shortcuts::Key::Enter,
            BevyKey::Tab => shortcuts::Key::Tab,
            BevyKey::F1 => shortcuts::Key::F1,
            BevyKey::Character(text) => match text.chars().next() {
                Some(c) => shortcuts::Key::Char(c.to_ascii_lowercase()),
                None => continue,
            },
            _ => continue,
        };
        let Some(shortcut) = shortcuts::shortcut(key, shift, model.0.side) else { continue };
        match shortcut {
            Shortcut::ReadHovered | Shortcut::MenuHovered => {
                let hovered = targets.iter().find_map(|(entity, interaction, click)| match (interaction, click) {
                    (Interaction::Hovered | Interaction::Pressed, Click::Target(target)) => Some((entity, target.clone())),
                    _ => None,
                });
                let Some((entity, target)) = hovered else { continue };
                if shortcut == Shortcut::ReadHovered {
                    pending.0.push(Intent::Inspect(target));
                } else {
                    let over = boxes.get(entity).map_or_else(|_| Anchor::default(), |(node, transform)| anchor_of(node, transform));
                    pending.0.push(Intent::Click { target, over });
                }
            }
            Shortcut::PlayHelper | Shortcut::PhaseBar => {
                if model.0.covered() {
                    continue;
                }
                let row = if shortcut == Shortcut::PhaseBar { Row::PhaseBar } else { Row::PlayHelper };
                settings_model::apply(&mut core.settings, settings_model::Intent::Toggle(row), &table::available(), &skin::available());
                if let Err(error) = core.save_settings() {
                    notices.push(format!("Settings not saved: {error}"));
                }
                // The phase panel is the right column's and costs the
                // cards nothing; the helper is the rail's.
                dirty.rail = true;
                dirty.side = true;
            }
            _ => pending.0.push(Intent::Shortcut(shortcut)),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn controls(
    mut commands: Commands,
    mut pressed: MessageReader<Pressed>,
    keys: Res<ButtonInput<KeyCode>>,
    faces: Query<(Entity, &Interaction, &Click), (Changed<Interaction>, Without<widgets::Themed>)>,
    boxes: Query<(&ComputedNode, &UiGlobalTransform)>,
    slots: Query<&HandSlot>,
    mut pending: ResMut<Pending>,
    marks: Query<&Click>,
    settings_marks: Query<&SettingsControl>,
    mut core: ResMut<ClientCore>,
    mut model: Option<ResMut<Model>>,
    active: Option<Res<ActiveMatch>>,
    mut dirty: ResMut<Dirty>,
    mut notices: ResMut<Notices>,
    mut navigate: MessageWriter<Navigate>,
    mut pace: Option<ResMut<Pace>>,
) {
    let mut intents: Vec<Intent> = std::mem::take(&mut pending.0);
    let mut leave_to: Option<AppScreen> = None;
    // Where the board's Menu and Quit lead: back to the tracks from a
    // lesson, which is where the next one is picked.
    let from_learn = active.as_ref().is_some_and(|active| active.lesson.is_some() || active.starter.is_some());
    let way_out = if from_learn { AppScreen::Learn } else { AppScreen::MainMenu };
    // A starter game is dealt again from here; a game from the form goes
    // back to the form, which reopens on its choice.
    let starter = active.as_ref().and_then(|active| active.starter);
    // With Ctrl or Cmd held the primary button is the secondary click
    // (`board_click` opened the sheet), so the press it also registers
    // on the card opens no menu.
    let modifier = secondary_modifier(&keys);
    // A press on a card or a zone is its menu, over the node's own box.
    let click_on = |entity: Entity, target: &Target| {
        let over = boxes.get(entity).map_or_else(|_| Anchor::default(), |(node, transform)| anchor_of(node, transform));
        Intent::Click { target: target.clone(), over }
    };
    // A face is a `Button` without the theme's recolouring, so the
    // shared feedback system never reports it; its press is read here.
    // The first hand-driven game found every card click doing nothing.
    for (entity, interaction, click) in &faces {
        if *interaction != Interaction::Pressed || modifier {
            continue;
        }
        match click {
            // A card of the person's own hand is `drag_hand`'s: its press
            // is armed there and comes back as this same click if the
            // pointer never moved.
            Click::Target(_) if slots.contains(entity) => {}
            Click::Target(target) => intents.push(click_on(entity, target)),
            // A card in the decision pop-up is its own button.
            Click::Entry(index) => intents.push(Intent::Choose(*index)),
            // The phase panel is unthemed like a face: a door to the
            // timing chart.
            Click::Timing => intents.push(Intent::ToggleTiming),
            // A card in a zone's sheet reads it large. This arm was
            // missing until Phase 7 §8 item 20, so the press did nothing.
            Click::Inspect(card) => intents.push(Intent::InspectCard(Some(card.clone()))),
            _ => {}
        }
    }
    for Pressed(entity) in pressed.read() {
        // A row of the options menu: the settings model applies it and
        // the file is saved, as on the settings screen; the rail and the
        // log follow the new value on the next redraw.
        if let Ok(SettingsControl::Intent(intent)) = settings_marks.get(*entity) {
            if settings_model::apply(&mut core.settings, intent.clone(), &table::available(), &skin::available()) {
                if let Err(error) = core.save_settings() {
                    notices.push(format!("Settings not saved: {error}"));
                }
                if let Some(pace) = pace.as_mut() {
                    pace.0.set_speed(core.settings.desktop.animation_speed);
                }
                dirty.rail = true;
                dirty.log = true;
                dirty.overlay = true;
                dirty.side = true;
            }
            continue;
        }
        match marks.get(*entity) {
            Ok(Click::Target(_)) if modifier => {}
            Ok(Click::Target(target)) => intents.push(click_on(*entity, target)),
            Ok(Click::Entry(index)) => intents.push(Intent::Choose(*index)),
            Ok(Click::Break(index)) => intents.push(Intent::Break(*index)),
            Ok(Click::TakeBack) => intents.push(Intent::TakeBack),
            Ok(Click::PassTheRun) => intents.push(Intent::PassTheRun),
            Ok(Click::Remember(answer)) => intents.push(Intent::Remember(*answer)),
            Ok(Click::Control(control)) => intents.push(Intent::Control(*control)),
            Ok(Click::Inspect(card)) => intents.push(Intent::InspectCard(Some(card.clone()))),
            Ok(Click::Stack(_)) if modifier => {}
            Ok(Click::Stack(server)) => intents.push(Intent::InspectStack(*server)),
            Ok(Click::Expand(row)) => intents.push(Intent::Expand(*row)),
            Ok(Click::Options) => intents.push(Intent::ToggleOptions),
            Ok(Click::Timing) => intents.push(Intent::ToggleTiming),
            Ok(Click::SaveReport) => {
                if let (Some(active), Some(model)) = (active.as_ref(), model.as_mut()) {
                    saved(&mut model.0, &mut notices, save_report(&core, &active.handle));
                    dirty.overlay = true;
                }
            }
            Ok(Click::WatchReport) => {
                if let Some(path) = model.as_ref().and_then(|model| model.0.saved_report_path.clone()) {
                    commands.insert_resource(OpenReplay(path, None));
                    leave_to = Some(AppScreen::Replay);
                }
            }
            Ok(Click::CloseOverlay) => intents.push(Intent::Back),
            Ok(Click::ConfirmQuit) => intents.push(Intent::ConfirmQuit),
            Ok(Click::CancelQuit) => intents.push(Intent::CancelQuit),
            Ok(Click::Quit) => intents.push(Intent::RequestQuit),
            Ok(Click::PlayAgain) if starter.is_none() => leave_to = Some(AppScreen::NewGame),
            Ok(Click::PlayAgain | Click::StarterGame) => {
                let side = model.as_ref().map_or(Side::Runner, |model| model.0.side);
                let again = starter.unwrap_or(crate::screens::new_game::Starter { side, boosted: false });
                match crate::screens::learn::start_starter(&core, again) {
                    Ok(next) => {
                        commands.insert_resource(NextMatch(next));
                        leave_to = Some(AppScreen::Game);
                    }
                    Err(error) => notices.push(format!("The starter game could not start: {error}")),
                }
            }
            Ok(Click::Menu | Click::Back) => leave_to = Some(way_out),
            Ok(Click::BeginLesson) => intents.push(Intent::BeginLesson),
            Ok(Click::EveryAction) => intents.push(Intent::EveryAction),
            Ok(Click::NextLesson | Click::RetryLesson) => {
                let Some(current) = active.as_ref().and_then(|active| active.lesson.as_ref()) else { continue };
                let lesson = if matches!(marks.get(*entity), Ok(Click::NextLesson)) { crate::screens::learn::next_after(&current.id) } else { Some(current.clone()) };
                match lesson.map(|lesson| crate::screens::learn::start(&core, lesson)) {
                    Some(Ok(next)) => {
                        commands.insert_resource(NextMatch(next));
                        leave_to = Some(AppScreen::Game);
                    }
                    Some(Err(error)) => notices.push(format!("The lesson could not start: {error}")),
                    None => leave_to = Some(AppScreen::Learn),
                }
            }
            Err(_) => {}
        }
    }
    if let Some(screen) = leave_to {
        navigate.write(Navigate(screen));
        return;
    }
    let Some(mut model) = model else { return };
    for intent in intents {
        let remembers = matches!(intent, Intent::Remember(_));
        // What a drag changes is on the board, not the rail: the hand's
        // order, the card picked up and the places lit for it. A redraw of
        // the rail alone left all three unchanged on the screen — the
        // model reordered the hand and the row showed the old order until
        // the next action happened to redraw it, and no place ever lit —
        // which is what "picking up and moving cards isn't working at all"
        // was (25 September 2026).
        // A release that never travelled is a click, which opens a menu
        // over the card and redraws only the rail, as any click does.
        let dragged = model.0.dragging.as_ref().is_some_and(|drag| drag.dragging);
        let moves_the_hand = match intent {
            Intent::DragMove { .. } | Intent::ReorderHand { .. } => true,
            Intent::DragDrop { .. } | Intent::DragRelease { .. } => dragged,
            _ => false,
        };
        let outcome = model.0.apply(intent);
        if moves_the_hand && outcome == Outcome::Redraw {
            dirty.board = true;
        }
        // An answer given for good is the settings file's, so the next
        // game, and the terminal, know it too.
        if remembers && matches!(outcome, Outcome::Submit(_)) && core.settings.answers != model.0.answers {
            core.settings.answers = model.0.answers.clone();
            if let Err(error) = core.save_settings() {
                notices.push(format!("Settings not saved: {error}"));
            }
        }
        match outcome {
            Outcome::Nothing => {}
            Outcome::Redraw => {
                dirty.rail = true;
                dirty.overlay = true;
            }
            // Never from a replay, which awaits nothing; and a match that
            // has gone has nobody to hand it to.
            Outcome::Submit(action) => {
                if let Some(Err(error)) = active.as_ref().map(|active| active.handle.submit(action)) {
                    notices.push(error);
                }
                dirty.rail = true;
                dirty.overlay = true;
            }
            Outcome::Rewind => {
                if let Some(Err(error)) = active.as_ref().map(|active| active.handle.rewind()) {
                    notices.push(error);
                }
                dirty.rail = true;
                dirty.overlay = true;
            }
            // A replay goes back to the list it was picked from, and a
            // lesson to the tracks.
            Outcome::Quit => {
                navigate.write(Navigate(if model.0.replay.is_some() { AppScreen::Replay } else { way_out }));
                return;
            }
        }
    }
}

fn redraw(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    model: Option<ResMut<Model>>,
    board: Query<Entity, With<Board>>,
    rail: Query<Entity, With<Rail>>,
    bar: Query<Entity, With<ControlBar>>,
    log: Query<Entity, With<LogList>>,
    mut log_row: Query<&mut Node, With<LogRow>>,
    mut log_scroll: Query<&mut ScrollPosition, With<LogScroll>>,
    // One query for the three floating layers: a system takes sixteen
    // parameters at most, and this one is at the limit.
    floating: Query<(Entity, Has<Overlay>, Has<DecisionPopup>, Has<ActionsMenu>), Or<(With<Overlay>, With<DecisionPopup>, With<ActionsMenu>)>>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    // The card pictures and the board's own, as one parameter: the system
    // is at Bevy's sixteen.
    (images, art, crops): (Res<CardImages>, Option<Res<BoardArt>>, Res<AvatarCrops>),
    fit: Option<Res<BoardFit>>,
) {
    let (Some(mut model), Some(fit)) = (model, fit) else { return };
    if !(dirty.board || dirty.rail || dirty.log || dirty.overlay) {
        return;
    }
    let Dirty { board: reboard, rail: rerail, log: relog, overlay: reoverlay, trail: _, side: _ } = std::mem::take(&mut *dirty);
    let game = &mut model.0;
    if reboard {
        let transitions = game.take_transitions();
        if let Ok(board) = board.single() {
            let art = art.as_deref();
            commands.entity(board).despawn_children().with_children(|parent| spawn_board(parent, &theme, &core, &images, art, &crops, game, &transitions, &fit));
        }
    }
    let prefs = &core.settings.desktop;
    if rerail {
        if let Ok(rail) = rail.single() {
            commands.entity(rail).despawn_children().with_children(|parent| spawn_rail(parent, &theme, game, prefs.play_helper));
        }
        // The bar is a row of the board, so a redrawn board brought a
        // fresh one with it; only a rail-only redraw refills the old one.
        if !reboard && let Ok(bar) = bar.single() {
            commands.entity(bar).despawn_children().with_children(|parent| spawn_control_bar(parent, &theme, game));
        }
        for (entity, _, _, _) in floating.iter().filter(|(_, _, popup, menu)| *popup || *menu) {
            commands.entity(entity).despawn();
        }
        let decisions = if game.awaiting && !game.finished() { game.actions.decisions() } else { Vec::new() };
        if let Some(root) = roots.iter().next() {
            if !decisions.is_empty() {
                commands.entity(root).with_children(|parent| spawn_decision_popup(parent, &theme, &core, &images, game, &decisions, fit.window));
            }
            if let Some(menu) = &game.menu {
                commands.entity(root).with_children(|parent| spawn_actions_menu(parent, &theme, game, menu, fit.window));
            }
        }
    }
    if relog {
        for mut node in &mut log_row {
            node.display = if prefs.play_history { Display::Flex } else { Display::None };
        }
        if prefs.play_history && let Ok(log) = log.single() {
            commands.entity(log).despawn_children().with_children(|parent| {
                for line in game.log.iter().rev().take(80).rev() {
                    spawn_log_line(parent, &theme, line);
                }
            });
            // The newest line is the one to read; the layout clamps this to
            // the real range.
            for mut position in &mut log_scroll {
                position.y = 1.0e6;
            }
        }
    }
    if reoverlay {
        for (overlay, _, _, _) in floating.iter().filter(|(_, overlay, _, _)| *overlay) {
            commands.entity(overlay).despawn();
        }
        if let Some(root) = roots.iter().next()
            && overlay_needed(game)
        {
            commands.entity(root).with_children(|parent| spawn_overlay(parent, &theme, &core, &images, game, fit.window));
        }
    }
}

fn status_line(game: &Game) -> String {
    let Some(view) = &game.view else { return "Setting up…".to_string() };
    let phase = match view.phase {
        GamePhase::Mulligan(side) => format!("{side:?} decides on the opening hand"),
        GamePhase::StartOfTurn(side) => format!("{side:?}'s turn begins"),
        GamePhase::Action(side) => format!("{side:?}'s turn"),
        GamePhase::Discard { side, .. } => format!("{side:?} discards"),
        GamePhase::GameOver(side) => format!("{side:?} wins"),
    };
    match game.replay {
        Some(_) => format!("Turn {} · {phase} · from the {:?}'s chair", view.turn, game.side),
        None => format!("Turn {} · {phase} · you are the {:?}", view.turn, game.side),
    }
}

fn overlay_needed(game: &Game) -> bool {
    game.covered()
}

/// A lesson's coach, at the head of the rail: which step, what it asks,
/// the hint, and the escape hatch. Above the prompt because it is what the
/// person reads first, and in the rail rather than a pop-up because it
/// stays up while they play — the terminal's coaching panel, beside the
/// board as it is there.
///
/// Drawn from the last coaching until the next arrives, so the words stay
/// while the opponent plays; the hatch is offered only while the person
/// is being asked, since it changes nothing else.
fn spawn_coaching(parent: &mut ChildSpawnerCommands, theme: &Theme, lesson: &LessonBoard, game: &Game) {
    let awaiting = game.awaiting;
    // Where the step's moves are made, off the narrowed map the board is
    // built from: only while the person is asked and the step narrows,
    // since with every action offered it would point at everything.
    let ways = match &game.view {
        Some(view) if awaiting && lesson.gated() && !lesson.every_action => {
            crate::models::lesson::ways(&game.actions, game.side).iter().map(|way| way.words(&game.actions, view, game.registry())).collect()
        }
        _ => Vec::new(),
    };
    let wrap = || TextLayout::new(Justify::Left, LineBreak::WordBoundary);
    parent
        .spawn((
            LessonCoach,
            Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(6), padding: UiRect::all(px(10)), border: UiRect::left(px(3)), ..default() },
            BackgroundColor(theme.glass),
            BorderColor::all(theme.accent),
        ))
        .with_children(|coach| {
            let Some(coaching) = &lesson.coaching else {
                coach.spawn((widgets::overline(theme, lesson.title.clone()), wrap()));
                return;
            };
            coach.spawn((widgets::overline(theme, format!("{} · step {} of {}", lesson.title, coaching.step, coaching.total)), wrap()));
            for paragraph in coaching.prose.split('\n').filter(|paragraph| !paragraph.is_empty()) {
                coach.spawn((widgets::label(theme, paragraph), wrap()));
            }
            if let Some(hint) = &coaching.hint {
                coach.spawn((Text::new(format!("Hint: {hint}")), theme.font(size::SMALL), TextColor(theme.accent), wrap()));
            }
            for way in &ways {
                coach.spawn((LessonWay, Text::new(way.clone()), theme.font(size::SMALL), TextColor(theme.text), wrap()));
            }
            if !awaiting {
                return;
            }
            if !lesson.gated() {
                coach.spawn((widgets::dim(theme, "Nothing this step asks for can be done right now, so every legal action is offered."), wrap()));
            } else if lesson.every_action {
                coach.spawn((widgets::dim(theme, "Every legal action is offered."), wrap()));
                coach.spawn(widgets::small_button(theme, ButtonKind::Secondary, "Only this step's actions", Click::EveryAction));
            } else {
                coach.spawn(widgets::small_button(theme, ButtonKind::Quiet, "Show every action", Click::EveryAction));
            }
        });
}

/// The coach's box, for a test to find.
#[derive(Component)]
pub struct LessonCoach;

/// One of the coach's "where" lines (`models::lesson::where_to`), for a
/// test to find.
#[derive(Component)]
pub struct LessonWay;

// ---- the board ----

#[allow(clippy::too_many_arguments)]
fn spawn_board(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, crops: &AvatarCrops, game: &Game, transitions: &[Transition], fit: &BoardFit) {
    let drag = game.dragged_slot();
    // The control bar, directly above the person's avatar bar and hand:
    // what they may do sits between what they hold and the table, and
    // acting never means crossing the opponent's side. A row of the
    // board, so it is redrawn with it; a rail-only redraw refills it in
    // place.
    let control_bar = |parent: &mut ChildSpawnerCommands, game: &Game| {
        parent
            .spawn((
                ControlBar,
                Node {
                    width: percent(100),
                    height: px(layout::CONTROL_BAR),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    ..default()
                },
            ))
            .with_children(|bar| spawn_control_bar(bar, theme, game));
    };
    let Some(view) = &game.view else {
        parent.spawn((widgets::dim(theme, "Waiting for the match to start…"), Node { flex_grow: 1.0, ..default() }));
        control_bar(parent, game);
        return;
    };
    let human = game.side;
    let opponent = human.other();
    let lit = Lit::of(transitions);
    // Top to bottom from either chair: the opponent's avatar bar, the
    // far area, the near area, the control bar, the person's avatar bar
    // over their hand. The opponent's hand is not drawn: a row of backs
    // said only how many they hold, which the Runner's Grip readout and
    // the Corp's HQ header say, and its height is the cards'. A server
    // column is a fixed number of strips (`layout::ServerWindow`), with
    // its plate on the Corp's edge; the rig's rows are reserved at their
    // size whether or not anything is in them and take the height the
    // window has spare, so nothing installed moves the middle. Each edge is one row of the board — the person's a column
    // of their bar and their hand, no gap between them — so the board's
    // gaps are as they were.
    parent.spawn(strip_row()).with_children(|edge| spawn_avatar_bar(edge, theme, core, crops, art, game, view, opponent, fit));
    spawn_area(parent, theme, core, images, art, game, view, opponent, &lit, fit);
    spawn_area(parent, theme, core, images, art, game, view, human, &lit, fit);
    control_bar(parent, game);
    // The person's edge is the board's last row and sits on the window's
    // bottom edge: the hand's peek touches it, as the opponent's bar
    // touches the top, and whatever height the rows leave is the middle
    // of the table's rather than under the hand. The hand comes
    // after the bar, so a card lifted out of it is drawn over the bar.
    parent.spawn(strip_row()).with_children(|edge| {
        spawn_avatar_bar(edge, theme, core, crops, art, game, view, human, fit);
        edge.spawn(centred_row()).with_children(|row| spawn_hand(row, theme, core, images, game, view, human, &lit, fit, drag));
    });
}

/// What the last transitions touched, so the redraw can outline it.
#[derive(Default)]
struct Lit {
    installs: Vec<InstallId>,
    hand: Vec<CardId>,
}

impl Lit {
    fn of(transitions: &[Transition]) -> Self {
        let mut lit = Lit::default();
        for transition in transitions {
            match transition {
                Transition::CardMoved { card, install, to, .. } => {
                    if let Some(id) = install {
                        lit.installs.push(*id);
                    }
                    if let (Some(card), Zone::Hand(_)) = (card, to) {
                        lit.hand.push(card.clone());
                    }
                }
                Transition::Revealed { install, .. } | Transition::Advancement { install, .. } => lit.installs.push(*install),
                _ => {}
            }
        }
        lit
    }
}

/// Lights an area a held card may be dropped on — the rig, the table —
/// as a server column is lit for an install: the accent round its edge
/// and a faint wash of it inside. An `Outline`, inset so a clipping
/// parent cannot cut it off, because an area is never a card and never
/// outlined for anything else; the wash is under the cards, which draw
/// over it.
fn welcome(area: &mut EntityCommands, theme: &Theme, welcomes: bool) {
    if welcomes {
        area.insert((Outline { width: px(2), offset: px(-2), color: theme.accent }, BackgroundColor(theme.accent.with_alpha(0.08))));
    }
}

fn outline(theme: &Theme) -> Outline {
    Outline { width: px(3), offset: px(1), color: theme.accent }
}

/// The glow that says the engine will accept something on this card, in
/// the mood `netrunner_client::board::affordance` gave it: purple for a
/// move at the person's own pace, yellow for a moment that will pass.
///
/// **A `BoxShadow` rather than an `Outline`, and not as a matter of
/// taste**: `Outline` is spoken for three times over on this board
/// already — the transition highlight, the dragged card's lift and the
/// ice being encountered — and a node has exactly one. A shadow is a
/// second component, so a card can glow *and* be outlined in the same
/// frame, which is the common case: the card just drawn is usually also
/// a card that can be played. Neither costs any layout, so `face_width`'s
/// budget and the no-scroll rule are untouched (AGENTS.md §5).
///
/// `BoxShadow` is a `Vec<ShadowStyle>`, so the contact shadows the board
/// is owed next (ROADMAP Phase 7, the third list's item 1) can be a
/// second entry in the same component rather than a second component
/// contending for the same slot.
fn glow(commands: &mut Commands, entity: Entity, theme: &Theme, mood: Option<Affordance>) {
    let Some(mood) = mood else { return };
    // A glowing button is made solid, in every state it can be drawn in,
    // or its own glow shows through it (`Theme::solid`). Queued on the
    // entity rather than done by `shadows`, so it lands in the same
    // buffer as the spawn: `widgets::dress` then only ever sees the solid
    // fill, where a later system's write raced it and lost.
    let ground = theme.solid(Color::NONE);
    let fills = move |drawn: Drawn| Drawn::new(crate::theme::over(drawn.bg, ground), drawn.border);
    commands.entity(entity).insert(Glowing(mood)).queue(move |mut entity: EntityWorldMut| {
        let Some(mut dressed) = entity.get_mut::<widgets::Dressed>() else { return };
        dressed.drawn = fills(dressed.drawn);
        dressed.hover = dressed.hover.map(fills);
        dressed.pressed = dressed.pressed.map(fills);
        let bg = dressed.drawn.bg;
        entity.insert(BackgroundColor(bg));
    });
}

/// Marks a glowing entity with its mood, so a test can ask the board
/// what it lit without reading a colour off a shadow.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glowing(pub Affordance);

/// Marks something that sits *on the table* — a card, a tile, a server
/// column — with the row it sits in, so it casts a contact shadow.
///
/// The board's chrome does not carry one: a control-bar button, a pop-up
/// or the phase bar is not an object on the field, and giving everything
/// a shadow is how a board ends up looking like a web page from 2013.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contact(pub Depth);

/// Composes the one `BoxShadow` a node may have from the two things that
/// want one: the contact shadow that sits it on the table, and the glow
/// that says the engine will accept something on it.
///
/// **They cannot be two components** — a node has exactly one `BoxShadow`
/// — and they must not be two `insert`s either, because the second would
/// silently replace the first. `BoxShadow` is a `Vec<ShadowStyle>`, so
/// this is the one place that builds the vector, and anything wanting a
/// third shadow later adds it here rather than contending for the slot.
///
/// Registered like `widgets::dress` and for the same reasons: on
/// `Added<..>` so a freshly spawned board is shadowed the frame after it
/// appears, and over everything when the [`Skin`] changes, because a skin
/// whose own pictures carry baked shadows turns the drawn ones off
/// (`Skin::wants_shadows`) and the settings row must take effect without
/// leaving the screen.
fn shadows(
    mut commands: Commands,
    theme: Res<Theme>,
    skin: Res<crate::skin::Skin>,
    added: Query<(Entity, Option<&Contact>, Option<&Glowing>), Or<(Added<Contact>, Added<Glowing>)>>,
    all: Query<(Entity, Option<&Contact>, Option<&Glowing>), Or<(With<Contact>, With<Glowing>)>>,
) {
    let wants = skin.wants_shadows();
    let compose = |commands: &mut Commands, entity: Entity, contact: Option<&Contact>, glowing: Option<&Glowing>| {
        let mut styles = Vec::new();
        // The glow goes first, which is the one that draws on top: a
        // halo the contact shadow has washed grey is not a signal.
        if let Some(Glowing(mood)) = glowing {
            let colour = match mood {
                Affordance::Usable => theme.glow_usable,
                Affordance::Conditional => theme.glow_conditional,
            };
            styles.push(ShadowStyle { color: colour, x_offset: px(0), y_offset: px(0), spread_radius: px(1), blur_radius: px(7) });
        }
        if let (Some(Contact(depth)), true) = (contact, wants) {
            let (y, blur, alpha) = depth.shadow();
            styles.push(ShadowStyle { color: Color::BLACK.with_alpha(alpha), x_offset: px(0), y_offset: px(y), spread_radius: px(0), blur_radius: px(blur) });
        }
        let mut entity = commands.entity(entity);
        if styles.is_empty() {
            entity.remove::<BoxShadow>();
        } else {
            entity.insert(BoxShadow(styles));
        }
    };
    if skin.is_changed() {
        for (entity, contact, glowing) in &all {
            compose(&mut commands, entity, contact, glowing);
        }
        return;
    }
    for (entity, contact, glowing) in &added {
        compose(&mut commands, entity, contact, glowing);
    }
}

fn section_label(parent: &mut ChildSpawnerCommands, theme: &Theme, text: impl Into<String>) {
    parent.spawn((widgets::dim(theme, text), Node { height: px(layout::LABEL), flex_shrink: 0.0, ..default() }));
}

/// A row that wraps: for a sheet's contents, where scrolling is the
/// overlay's and the board's rule does not apply.
fn wrap_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::FlexStart, row_gap: px(6), column_gap: px(6), ..default() }
}

/// A row of cards on the board that never wraps: too many overlap instead.
fn card_row() -> Node {
    Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, align_items: AlignItems::FlexStart, column_gap: px(layout::CARD_GAP), ..default() }
}

/// A side's edge of the table: its avatar bar and its hand, stacked with
/// no gap, as one row of the board.
fn strip_row() -> Node {
    Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, ..default() }
}

/// A hand's row: the hand centred under (or over) its side's avatar, with
/// the width of the board to spread into.
fn centred_row() -> Node {
    Node { width: percent(100), flex_shrink: 0.0, flex_direction: FlexDirection::Row, justify_content: JustifyContent::Center, ..default() }
}

/// Pulls the cards of a row together so `n` of them fit `available`:
/// every card after the first is moved left by the difference between
/// the natural advance and `layout::step`'s. Later cards draw over
/// earlier ones, as a held hand shows the right edge of each.
fn overlap(parent: &mut ChildSpawnerCommands, entities: &[Entity], width: f32, available: f32) {
    let step = layout::step(entities.len(), width, layout::CARD_GAP, available);
    let pull = step - (width + layout::CARD_GAP);
    if pull >= 0.0 {
        return;
    }
    for entity in entities.iter().skip(1) {
        parent.commands().entity(*entity).entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
    }
}

/// A side's avatar bar: the identity's art cropped to a disc in the
/// middle, the side's readouts either side of it on a plate of brushed
/// steel, lit for the side whose turn it is and grey for the other
/// (Phase 7 §4bi, the third list's item 2). It replaced the strip — a
/// small identity card at the left with its numbers beside it — which
/// the person found off to the side and dull.
///
/// **Nothing in it moves.** The row is the disc's height and the plate
/// the bar's, both constants of the chair (`layout::AVATAR`,
/// `layout::BAR`), so a new remote narrowing the cards, a tag or
/// a hand of ten leaves the bar and the avatar exactly where they were.
/// The wings share the board's width evenly whatever is on them.
///
/// **The disc is the identity's click.** Its actions, when the engine
/// offers any, are the menu a click opens; with none, the click reads the
/// identity whole, as the secondary click always does
/// (`Game::click`) — so the avatar is where the identity is read.
///
/// The pictures are board art (`avatar.bar`, `avatar.frame` and their
/// `.active` states, `board_art`), drawn in the three tiers: the bundled
/// steel, a person's own, or a plain plate washed in the side's colour.
/// The right wing is the left one mirrored.
#[allow(clippy::too_many_arguments)]
fn spawn_avatar_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, crops: &AvatarCrops, art: Option<&BoardArt>, game: &Game, view: &ClientView, side: Side, fit: &BoardFit) {
    let (disc, bar) = (layout::AVATAR, layout::BAR);
    let lit = view.active_player == side;
    let identity = match side {
        Side::Corp => view.corp.identity.clone(),
        Side::Runner => view.runner.identity.clone(),
    };
    let card = identity.as_ref().and_then(|id| core.registry.get(id));
    let colour = theme.side(side);
    let ink = if lit { theme.text } else { theme.text_dim };
    let state = if lit { colour } else { theme.text_dim };
    let key = |base: &str| if lit { format!("{base}.active") } else { base.to_string() };
    let wing_picture = art.and_then(|art| art.get(&key("avatar.bar")));
    let frame_picture = art.and_then(|art| art.get(&key("avatar.frame")));

    let readouts = hud::readouts(view, side);
    let (near_left, near_right) = readouts.split_at(readouts.len().div_ceil(2));
    let words = layout::bar_wing(fit.board_width(), disc) >= layout::BAR_WORDS_MIN;
    // The plate's text sits above the channel along its foot: the
    // picture's traces run in its lowest third.
    let over_the_channel = (bar * 0.3).round();
    let wing = |mirrored: bool| {
        let mut node = Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(0),
            height: px(bar),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            // A short wing packs its things closer, as it drops their
            // words: at 1366 × 768 the Runner's left wing is full to within
            // the gaps between them.
            column_gap: px(if words { 14.0 } else { 8.0 }),
            padding: UiRect { top: px(2), bottom: px(over_the_channel), ..default() },
            ..default()
        };
        // The outer end clears the plate's chamfer and the traces that
        // climb out of the channel there; the inner runs under the disc
        // and clears its ring.
        let (outer, inner) = (px(layout::BAR_OUTER), px(layout::BAR_TUCK + 12.0));
        if mirrored {
            node.margin.left = px(-layout::BAR_TUCK);
            node.padding.left = inner;
            node.padding.right = outer;
        } else {
            node.margin.right = px(-layout::BAR_TUCK);
            node.padding.left = outer;
            node.padding.right = inner;
        }
        node
    };
    // The plate's picture is a child filling the wing rather than the
    // wing's own image: a sliced image is drawn in its node's content box,
    // and the wing's padding — the chamfer, the channel, the disc — would
    // have shrunk the plate to the text's box.
    let dress = |wing: &mut ChildSpawnerCommands, mirrored: bool| {
        let fill = Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() };
        match wing_picture {
            Some(picture) => {
                let cap = picture.size.x * board_art::BAR_CAP as f32 / board_art::BAR_WIDTH as f32;
                wing.spawn((
                    ImageNode {
                        image_mode: NodeImageMode::Sliced(TextureSlicer {
                            border: BorderRect { min_inset: Vec2::new(cap, 0.0), max_inset: Vec2::new(cap, 0.0) },
                            center_scale_mode: SliceScaleMode::Stretch,
                            sides_scale_mode: SliceScaleMode::Stretch,
                            max_corner_scale: 1.0,
                        }),
                        flip_x: mirrored,
                        color: picture.tint(state),
                        ..ImageNode::new(picture.image.clone())
                    },
                    fill,
                    bevy::picking::Pickable::IGNORE,
                ));
            }
            // Headless, with no pictures: the plate as a fill, so a test
            // sees the same boxes.
            None => {
                wing.spawn((BackgroundColor(if lit { colour.with_alpha(0.25) } else { theme.panel }), fill, bevy::picking::Pickable::IGNORE));
            }
        }
    };

    parent
        .spawn((
            HudPanel(side),
            AvatarBar { side, lit },
            Node { width: percent(100), height: px(disc), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, ..default() },
        ))
        .with_children(|row| {
            // The left wing: who it is at the outer end, the first half
            // The left wing: who it is at the outer end — the name before
            // its subtitle, "Zahya Sadeghi" and "Haas-Bioroid" — and the
            // first half of the numbers against the disc.
            //
            // The Runner's two piles, zones a click opens as the Corp's
            // centrals are through their plates, go where there is room:
            // on a wide bar in the right wing beside the memory line, which
            // has the most to spare; on a short one in the left, in place
            // of the name, which a short wing cannot hold beside them — a
            // name clipped to "Zahya Sadegl" reads as a mistake, and the
            // avatar says who it is.
            let piles = |wing: &mut ChildSpawnerCommands| {
                if side != Side::Runner {
                    return;
                }
                for (pile, word, count) in [(Pile::Stack, "Stack", view.runner.stack_count.to_string()), (Pile::Heap, "Heap", view.runner.heap.len().to_string())] {
                    // A short wing drops the dot and the button's padding.
                    let label = if words { format!("{word} · {count}") } else { format!("{word} {count}") };
                    let entity = compact_button(wing, theme, label, Click::Target(Target::Pile(pile)));
                    if !words {
                        wing.commands().entity(entity).entry::<Node>().and_modify(|mut node| node.padding = UiRect::axes(px(8), px(4)));
                    }
                    glow(&mut wing.commands(), entity, theme, game.affordance_for(&Target::Pile(pile)));
                }
            };
            row.spawn(wing(false)).with_children(|wing| {
                dress(wing, false);
                if words || side == Side::Corp {
                    let name = card.map_or_else(|| format!("{side:?}"), |card| card.title.split(':').next().unwrap_or(&card.title).trim().to_string());
                    wing.spawn(Node { flex_shrink: 1.0, min_width: px(0), overflow: Overflow::clip(), ..default() }).with_children(|clip| {
                        clip.spawn((Text::new(name), theme.font(layout::BAR_WORD), TextColor(ink), TextLayout::new(Justify::Left, LineBreak::NoWrap)));
                    });
                }
                if !words {
                    piles(wing);
                }
                wing.spawn(Node { flex_grow: 1.0, ..default() });
                spawn_readouts(wing, theme, art, near_left, ink, words);
            });

            // The disc, over both wings' inner ends.
            let mut avatar = row.spawn((
                Avatar(side),
                Button,
                Click::Target(Target::Identity(side)),
                Node {
                    width: px(disc),
                    height: px(disc),
                    flex_shrink: 0.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                ZIndex(1),
                BackgroundColor(theme.faction(card.and_then(|card| card.faction)).darker(0.25)),
            ));
            let entity = avatar.id();
            avatar.with_children(|disc_node| {
                // The art sits inside the ring's inner edge.
                let inner = (disc * 0.86).round();
                match identity.as_ref().and_then(|id| crops.0.get(id)) {
                    Some((scan, rect)) => {
                        disc_node.spawn((
                            ImageNode { image_mode: NodeImageMode::Stretch, rect: Some(*rect), ..ImageNode::new(scan.clone()) },
                            Node { width: px(inner), height: px(inner), border_radius: BorderRadius::MAX, ..default() },
                            bevy::picking::Pickable::IGNORE,
                        ));
                    }
                    // No scan: the faction's mark in the icon font, else
                    // the identity's initial, on the faction's colour.
                    None => {
                        let mark = card.and_then(|card| card.faction).and_then(|faction| theme.faction_icon(faction, inner * 0.5));
                        let (text, font) = mark.unwrap_or_else(|| {
                            let initial = card.and_then(|card| card.title.chars().next()).unwrap_or('?');
                            (initial.to_string(), theme.font(inner * 0.45))
                        });
                        disc_node.spawn((Text::new(text), font, TextColor(theme.text), bevy::picking::Pickable::IGNORE));
                    }
                }
                if let Some(frame) = frame_picture {
                    disc_node.spawn((
                        ImageNode { image_mode: NodeImageMode::Stretch, color: frame.tint(state), ..ImageNode::new(frame.image.clone()) },
                        Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
                        bevy::picking::Pickable::IGNORE,
                    ));
                }
            });
            // An identity's own ability has nowhere else to live: it is
            // not an install and not in hand.
            glow(&mut row.commands(), entity, theme, game.affordance_for(&Target::Identity(side)));

            // The right wing: the rest of the numbers against the disc,
            // then — on a wide bar — the Runner's piles, and their memory
            // and link.
            row.spawn(wing(true)).with_children(|wing| {
                dress(wing, true);
                spawn_readouts(wing, theme, art, near_right, ink, words);
                wing.spawn(Node { flex_grow: 1.0, ..default() });
                if words {
                    piles(wing);
                }
                if let Some(details) = hud::details(view, side) {
                    wing.spawn((Text::new(details), theme.font(layout::BAR_WORD), TextColor(theme.text_dim), TextLayout::new(Justify::Left, LineBreak::NoWrap)));
                }
            });
        });
}

/// A button in the small size: seven server headers have to fit across
/// the board, and the shared button's body-size text does not.
fn compact_button(parent: &mut ChildSpawnerCommands, theme: &Theme, text: String, click: Click) -> Entity {
    parent
        .spawn((
            Button,
            widgets::Themed,
            click,
            Node {
                flex_shrink: 0.0,
                padding: UiRect::axes(px(12), px(6)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(theme.secondary),
            BorderColor::all(theme.glass_border),
            widgets::Dressed {
                slot: Slot::CompactButton,
                drawn: Drawn::new(theme.secondary, theme.glass_border),
                hover: Some(Drawn::new(theme.secondary_hover, theme.border_hover)),
                pressed: Some(Drawn::new(theme.secondary_press, theme.border_hover)),
            },
            children![(Text::new(text), theme.font(size::SMALL), TextColor(theme.text))],
        ))
        .id()
}

/// Readouts on the avatar bar, each a glyph, a number and its word on
/// one line: a side's numbers in `hud::readouts`' fixed order, so every
/// number keeps its place from one view to the next. The numbers were
/// sentences in the strip once (Phase 7 §4 item 6) — dim and small first,
/// nobody saw them; body size next, they were a line to read. A live
/// threat is drawn in the danger colour rather than added, which is
/// `hud`'s rule. A readout that opens a zone — Agendas, the score area —
/// is a button, drawn with the buttons' fill so it reads as one.
fn spawn_readouts(parent: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, readouts: &[hud::Readout], ink: Color, words: bool) {
    for readout in readouts {
        let colour = if readout.alarm { theme.danger } else { ink };
        let marker = HudReadout { label: readout.label, value: readout.value.clone() };
        let node = Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, align_items: AlignItems::Center, column_gap: px(4), ..default() };
        // A readout that opens something is a button; the rest are bare
        // numbers. Both are slots, so a skin can put a plate behind every
        // readout and a brighter one behind the door.
        let mut cell = match readout.opens {
            Some(pile) => parent.spawn((
                marker,
                Button,
                widgets::Themed,
                Click::Target(Target::Pile(pile)),
                Node { padding: UiRect::axes(px(8), px(1)), border_radius: BorderRadius::all(px(6)), ..node },
                BackgroundColor(theme.button),
                widgets::Dressed::button(theme, Slot::HudCellOpens, Drawn::new(theme.button, Color::NONE)),
            )),
            None => parent.spawn((marker, node, widgets::Dressed::still(if readout.alarm { Slot::HudCellAlarm } else { Slot::HudCell }, Drawn::new(Color::NONE, Color::NONE)))),
        };
        cell.with_children(|cell| {
            // The number, after Null Signal Games' own glyph for what it
            // counts when the board has one, and its word after it.
            let glyph = board_art::hud_key(readout.label).and_then(|key| art?.get(key));
            if let Some(picture) = glyph {
                cell.spawn(board_art::glyph(picture, if words { layout::BAR_GLYPH } else { layout::BAR_GLYPH_SHORT }));
            }
            cell.spawn((Text::new(readout.value.clone()), theme.font(if words { layout::BAR_NUMBER } else { layout::BAR_NUMBER_SHORT }), TextColor(colour)));
            // On a short wing the glyph stands for the word; a readout
            // with no glyph keeps its word, or it would be a bare number.
            if !words && glyph.is_some() {
                return;
            }
            cell.spawn((Text::new(readout.label), theme.font(layout::BAR_WORD), TextColor(if readout.alarm { theme.danger } else { theme.text_dim }), TextLayout::new(Justify::Left, LineBreak::NoWrap)));
        });
    }
}

/// The window a hand of `n` cards is seen through: `layout::PEEK` of a
/// card's height, clipping the rest. The cards stay whole and laid out at
/// their full size inside it — only what shows is cut — so a drag, a
/// click and the hover lift all measure the card, not the sliver.
///
/// Its width is the row's, overlap included, and it has to be said: a
/// clipping node contributes nothing to its parent's size, so left to
/// the layout the window was as wide as the "Your hand" label over it.
fn peek_window(size: FaceSize, n: usize, available: f32, depth: f32) -> Node {
    let width = if n == 0 { 0.0 } else { layout::step(n, size.width(), layout::CARD_GAP, available) * (n - 1) as f32 + size.width() };
    Node { width: px(width), height: px(depth), flex_shrink: 0.0, flex_direction: FlexDirection::Column, overflow: Overflow::clip(), ..default() }
}

/// A side's board: the Corp's servers, or the Runner's rig.
#[allow(clippy::too_many_arguments)]
fn spawn_area(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, game: &Game, view: &ClientView, side: Side, lit: &Lit, fit: &BoardFit) {
    let depth = Depth::area(side, game.side);
    match side {
        Side::Corp => spawn_servers(parent, theme, core, art, game, view, lit, fit, depth),
        Side::Runner => spawn_rig(parent, theme, core, images, art, game, view, lit, fit, depth),
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_servers(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, art: Option<&BoardArt>, game: &Game, view: &ClientView, lit: &Lit, fit: &BoardFit, depth: Depth) {
    // Archives, R&D, HQ, then the remotes, from either chair, every
    // central a column even with nothing on it (`board::table_servers`).
    let mut servers = netrunner_client::board::table_servers(view);
    // A card held over the board lights the places it may go, and a
    // remote the Corp has not made yet gets a column of its own for the
    // length of the drag: the engine offers it, and there was nothing to
    // drop on.
    let places = game.drop_places();
    for place in &places {
        if let Target::Server(server @ ServerId::Remote(_)) = place
            && !servers.iter().any(|s| s.server == *server)
        {
            servers.push(ServerView { server: *server, ice: Vec::new(), root: Vec::new() });
        }
    }
    let run = view.active_run.as_ref();
    let encountered = run.filter(|r| matches!(r.phase, RunPhase::ApproachIce | RunPhase::EncounterIce)).and_then(|r| r.ice.get(r.position)).map(|i| i.install_id);
    let size = fit.size_of(Side::Corp);
    // Every column is the same fixed number of strips, each the same
    // height (`layout::ServerWindow`): a row of columns whose strips
    // differed by column read as a bar chart, and a column that grew with
    // its ICE moved the rig.
    let height = layout::tile_height(size.width());
    let stack_height = layout::stack_height(size.width(), fit.slots);
    // The area takes what the rows leave over — the middle of the table,
    // once the rig's rows have grown to their cap — and its columns sit
    // on the Corp's edge of it, plates nearest the Corp. It is also the
    // table an event or an operation is dropped on, around and between
    // the columns, which take the drop only for what installs into them.
    let near_the_corp = if game.side == Side::Corp { JustifyContent::FlexEnd } else { JustifyContent::FlexStart };
    let mut area_node = parent.spawn((Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0), justify_content: near_the_corp, ..default() }, DropPlace::one(Target::Table)));
    welcome(&mut area_node, theme, places.contains(&Target::Table));
    area_node.with_children(|area| {
        section_label(area, theme, "Servers");
        let mut row_node = card_row();
        row_node.flex_shrink = 0.0;
        row_node.align_items = AlignItems::Stretch;
        area.spawn(row_node).with_children(|row| {
            for server in &servers {
                let under_run = game.run_on(server.server);
                let welcomes = places.contains(&Target::Server(server.server));
                let (border, slot) = if welcomes {
                    (theme.accent, Slot::ServerColumnWelcomes)
                } else if under_run {
                    (theme.runner, Slot::ServerColumnUnderRun)
                } else {
                    (theme.panel_border, Slot::ServerColumn)
                };
                row.spawn((
                    ServerColumn(server.server),
                    DropPlace::one(Target::Server(server.server)),
                    Node {
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        align_items: AlignItems::Center,
                        row_gap: px(4),
                        padding: UiRect::all(px(4)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(6)),
                        min_width: px(size.width() + 2.0 * layout::SERVER_CHROME - 2.0),
                        ..default()
                    },
                    // Mostly translucent, so the table shows through
                    // between the strips.
                    BackgroundColor(theme.panel.with_alpha(0.45)),
                    BorderColor::all(border),
                    widgets::Dressed::still(slot, Drawn::new(theme.panel.with_alpha(0.45), border)),
                    // The column is what sits on the table; its tiles sit
                    // on the column, one row nearer the light.
                    Contact(depth),
                ))
                .with_children(|column| {
                    // Plate, root and ice in the chair's order
                    // (`layout::column_top_down`): the plate nearest the
                    // Corp, the ice out toward the Runner, outermost
                    // nearest. The root and the ice share the stack, a
                    // fixed `slots` strips tall; what does not fit is the
                    // "+N" strip, between the root and the ICE shown —
                    // where the hidden inner ICE would be — and the whole
                    // server is its stack sheet.
                    let approached = encountered.and_then(|id| server.ice.iter().position(|ice| ice.install_id == id));
                    let shown = layout::ServerWindow::of(server.ice.len(), server.root.len(), root_lead(core, server), fit.slots, approached);
                    let pieces = layout::column_top_down(game.side);
                    let stack_node = Node {
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        height: px(stack_height),
                        row_gap: px(layout::TILE_GAP),
                        align_items: AlignItems::Center,
                        justify_content: if game.side == Side::Corp { JustifyContent::FlexEnd } else { JustifyContent::FlexStart },
                        overflow: Overflow::clip(),
                        ..default()
                    };
                    let spawn_stack = |column: &mut ChildSpawnerCommands| {
                        column.spawn(stack_node.clone()).with_children(|stack_parent| {
                            let more = |stack_parent: &mut ChildSpawnerCommands| {
                                if shown.hidden > 0 {
                                    spawn_more_strip(stack_parent, theme, server.server, shown.hidden, size, height);
                                }
                            };
                            for piece in pieces {
                                match piece {
                                    layout::Piece::Header => {}
                                    layout::Piece::Ice => {
                                        // The "+N" strip is on the root's
                                        // side of the ICE shown: under it
                                        // from the Corp's chair, over it
                                        // from the Runner's.
                                        if game.side == Side::Runner {
                                            more(stack_parent);
                                        }
                                        spawn_server_ice(stack_parent, theme, core, art, game, view, server, &shown.ice, game.side, encountered, lit, size, height, depth);
                                        if game.side == Side::Corp {
                                            more(stack_parent);
                                        }
                                    }
                                    layout::Piece::Root => spawn_server_root(stack_parent, theme, core, art, game, view, server, &shown.root, lit, size, height, depth),
                                }
                            }
                        });
                    };
                    if game.side == Side::Corp {
                        spawn_stack(column);
                        spawn_server_plate(column, theme, art, view, server.server, size, welcomes, under_run, game.affordance_for(&Target::Server(server.server)));
                    } else {
                        spawn_server_plate(column, theme, art, view, server.server, size, welcomes, under_run, game.affordance_for(&Target::Server(server.server)));
                        spawn_stack(column);
                    }
                });
            }
        });
    });
}

/// A server's nameplate: its name and count in a frame (`board_art`'s
/// `plate.*`, nine-sliced as a strip is) on the Corp's edge of the
/// table, `layout::plate_height` tall — a strip's height, so the plate
/// heads its column's stack. Under a run it shows the server's `.run`
/// frame, lit in the Runner's colour. A click is the server's, as the
/// header's was.
///
/// It was a 16:9 picture of a building until 25 September 2026, when the
/// person asked for the height back for the installs.
#[allow(clippy::too_many_arguments)]
fn spawn_server_plate(column: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, view: &ClientView, server: ServerId, size: FaceSize, welcomes: bool, under_run: bool, mood: Option<Affordance>) {
    let on_board = view.corp.servers.iter().any(|s| s.server == server) || !matches!(server, ServerId::Remote(_));
    let count = match server {
        ServerId::Hq => format!(" · {}", view.corp.hq_count),
        ServerId::RnD => format!(" · {}", view.corp.rd_count),
        ServerId::Archives => format!(" · {}", view.corp.archives.len()),
        ServerId::Remote(_) => String::new(),
    };
    // The column a drag conjured says what it is: a server that does not
    // exist yet, named for what dropping there would make.
    let label = if on_board { format!("{}{count}", server_name(server)) } else { format!("New {}", server_name(server).to_lowercase()) };
    // A plate is a button wearing the header's hat: its own slot, so a
    // skin can draw Archives unlike a Stack button.
    let slot = if welcomes { Slot::ServerHeaderWelcomes } else { Slot::ServerHeader };
    let (width, height) = (size.width() + 4.0, layout::plate_height(size.width()));
    let mut plate = column.spawn((
        ServerPlate(server),
        Button,
        widgets::Themed,
        Click::Target(Target::Server(server)),
        Node {
            width: px(width),
            height: px(height),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(theme.panel_border),
        widgets::Dressed::button(theme, slot, Drawn::new(theme.button, theme.panel_border)),
    ));
    plate.with_children(|plate| {
        // The frame first, so the name draws over it; inside the border.
        // A drawn frame is painted in the Corp's colour already.
        let key = board_art::server_key(server, under_run);
        if let Some(picture) = art.and_then(|art| art.get(key)) {
            plate.spawn((PlateFrame(key), board_art::strip(picture, Vec2::new(width - 2.0, height - 2.0), Color::WHITE)));
        }
        spawn_plate_name(plate, theme, label, (size::SMALL - 1.0).min(height * 0.5));
    });
    if welcomes {
        plate.insert(outline(theme));
    }
    let entity = plate.id();
    // A server glows for the run or the install the engine offers on it,
    // which is the only affordance on the board with no card to carry it.
    glow(&mut column.commands(), entity, theme, mood);
}

/// A nameplate's name, one line on a band of the panel inside the
/// frame's channel, so it reads over any frame and a long name is cut
/// at the plate's edge rather than wrapped out of it.
fn spawn_plate_name(plate: &mut ChildSpawnerCommands, theme: &Theme, name: String, text_size: f32) {
    plate
        .spawn((
            Node { justify_content: JustifyContent::Center, padding: UiRect::axes(px(6), px(1)), border_radius: BorderRadius::all(px(3)), max_width: percent(100), min_width: px(0), overflow: Overflow::clip(), ..default() },
            BackgroundColor(theme.panel.with_alpha(0.6)),
            Pickable::IGNORE,
        ))
        .with_children(|band| {
            band.spawn((Text::new(name), theme.font(text_size), TextColor(theme.text), TextLayout::no_wrap(), Pickable::IGNORE));
        });
}

/// A rig row's nameplate at the row's left — "Programs", "Hardware",
/// "Resources" — in the same frame a server's name sits in, a strip
/// tall at the top of the row, so every name on the table is dressed
/// alike. It is a label, never a button: nothing is done to a row.
fn spawn_rig_plate(row: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, wanted: netrunner_client::board::rig::RigRow, height: f32) {
    row.spawn(Node { width: px(layout::RIG_LABEL_WIDTH), flex_shrink: 0.0, padding: UiRect::right(px(6)), ..default() }).with_children(|slot| {
        slot.spawn(Node { width: percent(100), height: px(height), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }).with_children(|plate| {
            let key = board_art::rig_key(wanted);
            match art.and_then(|art| art.get(key)) {
                Some(picture) => {
                    plate.spawn((PlateFrame(key), board_art::strip(picture, Vec2::new(layout::RIG_LABEL_WIDTH - 6.0, height), Color::WHITE)));
                    spawn_plate_name(plate, theme, wanted.label().to_string(), (size::SMALL - 3.0).min(height * 0.45));
                }
                // Headless, with no pictures: the words, as they were.
                None => {
                    plate.spawn(widgets::dim(theme, wanted.label()));
                }
            }
        });
    });
}

/// A token beside a number: the kind's glyph when the board has one
/// (`board_art::token_key`, falling back to a drawn disc), and the words
/// alone when it has no pictures at all — the headless tests, which read
/// the same `2/3 adv` the terminal client prints.
fn spawn_badge(parent: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, token: &Token, glyph: f32, text_size: f32, colour: Color) {
    let key = board_art::token_key(token.kind);
    match art.and_then(|art| art.get(key)) {
        Some(picture) => {
            parent
                .spawn((TokenBadge(key), Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(2), flex_shrink: 0.0, ..default() }))
                .with_children(|badge| {
                    badge.spawn(board_art::glyph(picture, glyph));
                    badge.spawn((Text::new(token.amount.clone()), theme.font(text_size), TextColor(colour)));
                });
        }
        None => {
            parent.spawn((TokenBadge(key), Text::new(token.words()), theme.font(text_size), TextColor(colour)));
        }
    }
}

/// What a tile shows: its words, its tokens, its picture and the colour
/// of its state — the border's, and what a drawn picture is washed in.
struct TileLook {
    title: String,
    tokens: Vec<Token>,
    key: &'static str,
    colour: Color,
    text_colour: Color,
    /// The Trojans hosted on an ice, each its own button on the band;
    /// none on a root card.
    hosted: Vec<Hosted>,
}

/// A Trojan as its host's tile shows it: its title, its counters, and
/// the Trojan's own mood and lit state.
struct Hosted {
    install: InstallId,
    title: String,
    counters: Option<Token>,
    mood: Option<Affordance>,
    lit: bool,
}

/// A tile in a server column — an ice or a root card: a picture of what
/// kind of card it is (`board_art`: a face-down card, a barrier, an
/// asset…) behind the title when it may be named, its tokens as badges,
/// and a frame lit in its kind's colour (`Theme::tile`: gold for a
/// barrier, blue for a code gate, red for a sentry, grey face down). A
/// click opens the card's menu and a secondary click its sheet; the
/// card's own picture is read there, not here, so a column is strips
/// `layout::tile_height` tall and never a face.
#[allow(clippy::too_many_arguments)]
fn spawn_tile(column: &mut ChildSpawnerCommands, theme: &Theme, art: Option<&BoardArt>, look: TileLook, install: InstallId, lit: bool, size: FaceSize, height: f32, slot: Slot, mood: Option<Affordance>, depth: Depth) -> Entity {
    let width = size.width() + 4.0;
    let mut tile = column.spawn((
        Button,
        widgets::Themed,
        Click::Target(Target::Install(install)),
        DropPlace::one(Target::Install(install)),
        Node {
            width: px(width),
            height: px(height),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            column_gap: px(4),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            overflow: Overflow::clip(),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(theme.button),
        BorderColor::all(look.colour),
        // The border carries the state — a rezzed ice is its faction's
        // colour — so that colour is what a picture asking for `"state"`
        // is washed in.
        widgets::Dressed::button(theme, slot, Drawn::new(theme.button, look.colour)),
    ));
    tile.with_children(|tile| {
        if let Some(picture) = art.and_then(|art| art.get(look.key)) {
            tile.spawn((TileArt(look.key), board_art::strip(picture, Vec2::new(width - 2.0, height - 2.0), look.colour)));
        }
        // The words on a band, so they read over any picture; the badges
        // beside them at the tile's text size.
        let text_size = size::SMALL - 3.0;
        // Never wider than the strip, and clipped at its right: a band
        // wider than a narrow column was centred, and lost the start of
        // the name as well as the end of the state.
        tile.spawn((
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::axes(px(6), px(1)), border_radius: BorderRadius::all(px(3)), max_width: percent(100), min_width: px(0), flex_shrink: 1.0, overflow: Overflow::clip(), ..default() },
            BackgroundColor(theme.panel.with_alpha(0.7)),
        ))
        .with_children(|band| {
            // One line, never wrapped: a strip is `layout::tile_height`
            // tall, and a narrow column (a server is ~105 px from the
            // Runner's chair at 1366 × 768) wrapped "Manegarm Skunkworks
            // · rezzed" into three lines the strip clipped. Unwrapped, the
            // name reads first and what the column cuts off is the state,
            // which the frame's colour says too.
            // In a box of its own that clips: a text node does not clip its
            // own glyphs, and the words ran under the badges beside them.
            band.spawn(Node { flex_shrink: 1.0, min_width: px(0), overflow: Overflow::clip(), ..default() }).with_children(|words| {
                words.spawn((Text::new(look.title), theme.font(text_size), TextColor(look.text_colour), TextLayout::no_wrap()));
            });
            for token in &look.tokens {
                spawn_badge(band, theme, art, token, (height - 8.0).clamp(12.0, 18.0), text_size, look.text_colour);
            }
        });
        // A Trojan sits on its ice: a button inside the tile's, whose
        // `FocusPolicy::Block` (a `Button`'s) keeps the press and the
        // hover from the tile, so a click on it is the Trojan's menu and
        // a secondary click its sheet. Beside the band on the strip's one
        // line: a strip is `layout::tile_height` tall, which has no room
        // for a second.
        if look.hosted.is_empty() {
            return;
        }
        tile.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(3), ..default() }).with_children(|line| {
            for hosted in &look.hosted {
                let mut chip = line.spawn((
                    Button,
                    HostedChip(hosted.install),
                    Click::Target(Target::Install(hosted.install)),
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(3), flex_shrink: 0.0, padding: UiRect::axes(px(4), px(0)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), ..default() },
                    BackgroundColor(theme.button),
                    BorderColor::all(theme.runner),
                ));
                chip.with_children(|chip| {
                    chip.spawn((Text::new(hosted.title.clone()), theme.font(text_size), TextColor(theme.text), Pickable::IGNORE));
                    if let Some(token) = &hosted.counters {
                        spawn_badge(chip, theme, art, token, (height - 8.0).clamp(12.0, 16.0), text_size, theme.text);
                    }
                });
                if hosted.lit {
                    chip.insert(outline(theme));
                }
                let entity = chip.id();
                glow(&mut line.commands(), entity, theme, hosted.mood);
            }
        });
    });
    if lit {
        tile.insert(outline(theme));
    }
    let entity = tile.id();
    column.commands().entity(entity).insert(Contact(depth.nearer()));
    glow(&mut column.commands(), entity, theme, mood);
    entity
}

/// A server's ice as tiles, titled by `board::facts::tile_title` — the
/// title when it may be named, rezzed or unrezzed, its strength now —
/// with its tokens as badges (`facts::tile_tokens`) and the run's
/// marker on the piece being approached.
#[allow(clippy::too_many_arguments)]
fn spawn_server_ice(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, art: Option<&BoardArt>, game: &Game, view: &ClientView, server: &ServerView, shown: &std::ops::Range<usize>, chair: Side, encountered: Option<InstallId>, lit: &Lit, size: FaceSize, height: f32, depth: Depth) {
    for ice in layout::ice_top_down(&server.ice[shown.clone()], chair) {
        let def = ice.card.as_ref().and_then(|id| core.registry.get(id));
        let kind = def.and_then(|d| match &d.card_type {
            CardType::Ice(kind) => Some(*kind),
            _ => None,
        });
        let key = board_art::ice_key(ice.rezzed, kind);
        let look = TileLook {
            title: facts::tile_title(view, ice.install_id, &core.registry),
            tokens: facts::tile_tokens(view, ice.install_id, &core.registry),
            key,
            colour: theme.tile(key),
            text_colour: if ice.rezzed { theme.text } else { theme.text_dim },
            hosted: netrunner_client::board::rig::hosted_on(view, ice.install_id)
                .into_iter()
                .map(|trojan| {
                    let def = core.registry.get(&trojan.card);
                    Hosted {
                        install: trojan.install_id,
                        title: def.map_or_else(|| trojan.card.0.clone(), |def| def.title.clone()),
                        counters: (trojan.counters > 0).then(|| Token { kind: TokenKind::Counter(def.and_then(|d| d.counter_kind)), amount: trojan.counters.to_string() }),
                        mood: game.affordance_for(&Target::Install(trojan.install_id)),
                        lit: lit.installs.contains(&trojan.install_id),
                    }
                })
                .collect(),
        };
        let is_lit = encountered == Some(ice.install_id) || lit.installs.contains(&ice.install_id);
        let slot = if ice.rezzed { Slot::TileRezzed } else { Slot::TileUnrezzed };
        spawn_tile(column, theme, art, look, ice.install_id, is_lit, size, height, slot, game.affordance_for(&Target::Install(ice.install_id)), depth);
    }
}

/// The cards in a server's root as tiles, titled by
/// `board::facts::tile_title`: rezzed or unrezzed for an asset or an
/// upgrade, the title alone for an agenda the viewer knows, `face down`
/// for a card the viewer cannot name — with its advancement (public) and
/// counters as badges. An agenda is lit face up: it has no rez to wait
/// for.
#[allow(clippy::too_many_arguments)]
fn spawn_server_root(column: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, art: Option<&BoardArt>, game: &Game, view: &ClientView, server: &ServerView, shown: &[usize], lit: &Lit, size: FaceSize, height: f32, depth: Depth) {
    for card in shown.iter().filter_map(|&i| server.root.get(i)) {
        let def = card.card.as_ref().and_then(|id| core.registry.get(id));
        let face_up = card.rezzed || def.is_some_and(|d| d.card_type == CardType::Agenda);
        let key = board_art::root_key(face_up, def.map(|d| &d.card_type));
        let look = TileLook {
            title: facts::tile_title(view, card.install_id, &core.registry),
            tokens: facts::tile_tokens(view, card.install_id, &core.registry),
            key,
            colour: theme.tile(key),
            text_colour: if face_up { theme.text } else { theme.text_dim },
            hosted: Vec::new(),
        };
        let slot = if face_up { Slot::TileRezzed } else { Slot::TileUnrezzed };
        spawn_tile(column, theme, art, look, card.install_id, lit.installs.contains(&card.install_id), size, height, slot, game.affordance_for(&Target::Install(card.install_id)), depth);
    }
}

/// The root card a column folded to one strip shows: its asset or
/// agenda, the card the server is there to protect, else its first card
/// — an upgrade, or a card the viewer cannot name.
fn root_lead(core: &ClientCore, server: &ServerView) -> usize {
    server
        .root
        .iter()
        .position(|card| card.card.as_ref().and_then(|id| core.registry.get(id)).is_some_and(|def| matches!(def.card_type, CardType::Asset | CardType::Agenda)))
        .unwrap_or(0)
}

/// The "+N" strip: the cards of a server its column has no strip for,
/// counted, and the door to the whole server in run order (the stack
/// sheet) on either button. A strip's size and a quiet look, so a column
/// reads as the same stack of strips with one of them a count.
fn spawn_more_strip(column: &mut ChildSpawnerCommands, theme: &Theme, server: ServerId, hidden: usize, size: FaceSize, height: f32) {
    column
        .spawn((
            Button,
            widgets::Themed,
            MoreStrip(server),
            Click::Stack(server),
            Node {
                width: px(size.width() + 4.0),
                height: px(height),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            BackgroundColor(theme.button),
            BorderColor::all(theme.panel_border),
            widgets::Dressed::button(theme, Slot::TileUnrezzed, Drawn::new(theme.button, theme.panel_border)),
        ))
        .with_children(|strip| {
            strip.spawn((Text::new(format!("+{hidden} more")), theme.font(size::SMALL - 1.0), TextColor(theme.text), Pickable::IGNORE));
        });
}

/// The rig as three rows — programs, hardware, resources — in the order
/// the chair sees the table (`netrunner_client::board::rig::rows_top_down`: programs the row
/// nearest the ICE), each the top of its cards — `layout::PEEK` of a
/// card, deeper when the window has height to spare — over their chip
/// line, overlapped by `layout::step` when a row would not fit across.
/// Drawn at the Runner's side of the table's size — full from the
/// Runner's chair, `layout::OPPONENT_RIG_SCALE` from the Corp's — with
/// every row reserved at `layout::rig_row_height` whether or not anything
/// is in it, so the first install moves nothing. A row's label sits at its left
/// rather than over it: the rig has width to spare and no height.
///
/// **A row is a row of stacks** (`netrunner_client::board::rig::stacked`):
/// copies nothing tells apart are one face, its first copy's, with the
/// count first on its chip line, so three Docklands Passes take one card's
/// width and the row overlaps later. The count is on the chip line and not
/// on the face because a scan replaces a text face's children when it
/// lands (`card_images`), and a badge drawn there went with them.
#[allow(clippy::too_many_arguments)]
fn spawn_rig(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, art: Option<&BoardArt>, game: &Game, view: &ClientView, lit: &Lit, fit: &BoardFit, depth: Depth) {
    let size = fit.size_of(Side::Runner);
    let row_height = layout::rig_row_height(fit.area_face(Side::Runner), fit.spare);
    // The peek is the row less its chip line: a third of a card at least,
    // more when the window has the height.
    let peek = row_height - layout::CHIPS;
    let available = fit.board_width() - layout::RIG_LABEL_WIDTH;
    // The rig is where a Runner install is dropped, and part of the table
    // an event is dropped on.
    let places = game.drop_places();
    let mut rig = parent.spawn((
        Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, row_gap: px(layout::RIG_ROW_GAP), height: px(layout::rig_height(fit.area_face(Side::Runner), fit.spare)), overflow: Overflow::clip(), ..default() },
        DropPlace(vec![Target::Rig, Target::Table]),
    ));
    welcome(&mut rig, theme, places.contains(&Target::Rig) || places.contains(&Target::Table));
    rig.with_children(|area| {
        for (wanted, stacks) in netrunner_client::board::rig::stacked(view, &core.registry, fit.chair, Some(&game.actions)) {
            let cards: Vec<_> = stacks.iter().map(|stack| stack.first()).collect();
            let step = layout::step(cards.len(), size.width(), layout::CARD_GAP, available);
            let pull = (step - (size.width() + layout::CARD_GAP)).min(0.0);
            area.spawn((Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, height: px(row_height), ..default() },)).with_children(|row| {
                spawn_rig_plate(row, theme, art, wanted, layout::tile_height(fit.area_face(Side::Runner)));
                row.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|column| {
                    column.spawn(peek_window(size, cards.len(), available, peek)).with_children(|window| {
                        window.spawn(card_row()).with_children(|cards_row| {
                            for (i, card) in cards.iter().enumerate() {
                                let Some(def) = core.registry.get(&card.card) else { continue };
                                let image = def.numeric_id.and_then(|code| images.face(code, size));
                                let entity = spawn_face(cards_row, theme, &Face::of(def), size, image, (Button, Click::Target(Target::Install(card.install_id))));
                                if i > 0 && pull < 0.0 {
                                    cards_row.commands().entity(entity).entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
                                }
                                cards_row.commands().entity(entity).insert(Contact(depth));
                                if netrunner_client::board::rig::is_ghost(card) {
                                    cards_row.commands().entity(entity).insert(Ghost(card.install_id)).with_children(|face| {
                                        face.spawn((GhostWash, Pickable::IGNORE, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), right: px(0), bottom: px(0), ..default() }, BackgroundColor(theme.background.with_alpha(1.0 - GHOST_ALPHA))));
                                    });
                                }
                                glow(&mut cards_row.commands(), entity, theme, game.affordance_for(&Target::Install(card.install_id)));
                                if stacks[i].install_ids().any(|id| lit.installs.contains(&id)) {
                                    cards_row.commands().entity(entity).insert(outline(theme));
                                }
                            }
                        });
                    });
                    // The chip line under the peek, one slot per card at the
                    // row's own step, so a card's numbers sit under it:
                    // strength and hosted cards as words, the counters as
                    // their kind's badge (a virus program's virus counters,
                    // a credit resource's credits).
                    column.spawn(Node { flex_direction: FlexDirection::Row, flex_shrink: 0.0, column_gap: px(layout::CARD_GAP), height: px(layout::CHIPS), ..default() }).with_children(|line| {
                        for (i, card) in cards.iter().enumerate() {
                            let Some(def) = core.registry.get(&card.card) else { continue };
                            let mut slot = line.spawn(Node { width: px(size.width()), flex_shrink: 0.0, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, justify_content: JustifyContent::Center, column_gap: px(8), overflow: Overflow::clip(), ..default() });
                            if i > 0 && pull < 0.0 {
                                slot.entry::<Node>().and_modify(move |mut node| node.margin.left = px(pull));
                            }
                            let mut chips = Vec::new();
                            let copies = stacks[i].count();
                            if def.card_type == CardType::Program && def.strength.is_some() {
                                chips.push(format!("str {}", card.current_strength));
                            }
                            if !card.hosted_cards.is_empty() {
                                chips.push(format!("{} hosted", card.hosted_cards.len()));
                            }
                            if let Some(host) = card.hosted_on_ice {
                                chips.push(format!("on {}", netrunner_client::board::rig::host_label(view, &core.registry, host)));
                            }
                            slot.with_children(|slot| {
                                if copies > 1 {
                                    slot.spawn((RigCopies(copies), Text::new(format!("×{copies}")), theme.font(size::SMALL), TextColor(theme.text)));
                                }
                                if !chips.is_empty() {
                                    slot.spawn(widgets::dim(theme, chips.join(" · ")));
                                }
                                if card.counters > 0 {
                                    let token = Token { kind: TokenKind::Counter(def.counter_kind), amount: card.counters.to_string() };
                                    spawn_badge(slot, theme, art, &token, 16.0, size::SMALL, theme.text_dim);
                                }
                            });
                        }
                    });
                });
            });
        }
    });
}

/// A ghost's scan drawn at `GHOST_ALPHA`, whether it was the face from
/// the start or landed in place of the text face and its wash. Every
/// frame rather than on `Changed<ImageNode>`: there are one or two
/// ghosts on a table, and the write is skipped when it would change
/// nothing, so no swap of a scan can slip past it.
fn fade_ghosts(mut ghosts: Query<&mut ImageNode, With<Ghost>>) {
    for mut image in &mut ghosts {
        if image.color.alpha() != GHOST_ALPHA {
            image.color.set_alpha(GHOST_ALPHA);
        }
    }
}

/// The person's hand under their avatar bar, the top `layout::PEEK` of each
/// card showing and the rest below the table's edge, overlapped when it
/// is wide. A hovered card rises out of the row whole (`raise_hand`).
#[allow(clippy::too_many_arguments)]
fn spawn_hand(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side, lit: &Lit, fit: &BoardFit, drag: Option<usize>) {
    // The person's own order for their own hand; the view's for any
    // other, though only the person's is drawn.
    let own = side == game.side;
    let from_view = match side {
        Side::Corp => view.corp.hq_cards.as_deref(),
        Side::Runner => view.runner.grip_cards.as_deref(),
    }
    .unwrap_or(&[]);
    let hand = if own { game.hand.cards() } else { from_view };
    let size = fit.size_of(side);
    let available = fit.board_width();
    parent.spawn((Node { flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() },)).with_children(|column| {
        // No "Your hand · N" over it: the Runner's Grip readout is on the
        // bar and the Corp's HQ header carries the count, and the label's
        // line was height the hand's own row did not need.
        column.spawn(peek_window(size, hand.len(), available, (layout::PEEK * size.height()).round())).with_children(|window| {
            window.spawn(card_row()).with_children(|row| {
                // A hand is a multiset; the first copy of a lit card is the
                // one outlined, which is as much as a highlight can say.
                let mut lit_left = lit.hand.clone();
                let mut faces = Vec::new();
                for (slot, id) in hand.iter().enumerate() {
                    let Some(def) = core.registry.get(id) else { continue };
                    let image = def.numeric_id.and_then(|code| images.face(code, size));
                    let entity = spawn_face(row, theme, &Face::of(def), size, image, (Button, Click::Target(Target::HandCard(id.clone()))));
                    if own {
                        // Its place in the row, so a drag knows which card
                        // it picked up and where the others sit.
                        row.commands().entity(entity).insert(HandSlot(slot));
                    }
                    row.commands().entity(entity).insert(Contact(Depth::strip(side, game.side)));
                    if own {
                        glow(&mut row.commands(), entity, theme, game.affordance_for(&Target::HandCard(id.clone())));
                    }
                    if let Some(at) = lit_left.iter().position(|c| c == id) {
                        lit_left.swap_remove(at);
                        row.commands().entity(entity).insert(outline(theme));
                    }
                    // The card being dragged is lifted out of the row by its
                    // outline: the row itself never moves under the pointer,
                    // because a hand that re-flowed mid-drag moved the gap
                    // the person was aiming at.
                    if own && drag.is_some_and(|dragged| dragged == slot) {
                        row.commands().entity(entity).insert(outline(theme));
                    }
                    faces.push(entity);
                }
                overlap(row, &faces, size.width(), available);
            });
        });
    });
}

/// The hand card that is out of the row: raised whole by a hover, or
/// following the pointer in a drag (`raise_hand`). On the face itself —
/// there is no copy.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiftedCard;

/// A hand shows the top of each card (`layout::PEEK`); the card the
/// pointer rests on **rises out of the row** until it is whole, and a card
/// picked up **follows the pointer** until it is put down.
///
/// **The card itself moves, not a copy of it.** The first cut drew a
/// second, inert copy above the row, so the face in the row never moved
/// and a drag measured the same box it always had; to the person it was a
/// picture laid over the strip, which looked wrong, and a drag that moved
/// nothing on the screen read as a drag that did not work (reported 25
/// September 2026: "picking up and moving cards isn't working at all").
/// So the face is moved by a `UiTransform` — which moves what is drawn
/// and what is hit together, without laying the row out again, so its
/// neighbours hold still — freed from the strip's clip by `OverrideClip`,
/// and drawn over the board by a `GlobalZIndex`.
///
/// **A hover raises it by exactly the part the strip hides**, so its
/// bottom sits on the window's edge and it still covers the whole of its
/// place in the strip: a pointer anywhere on the strip is still on the
/// card, and it does not drop and rise again at its lower edge. **A drag
/// carries it from where the press found it**, raised, by the distance the
/// pointer has travelled, so the point that was grabbed stays under the
/// pointer. A card whose menu is open stays raised, so the menu sits over
/// the card it belongs to.
///
/// It is paint and position, never play: nothing here enters a
/// `PlayerAction`, the log or a `ClientView`, and the order a drag leaves
/// is the model's (`Intent::DragRelease`).
#[allow(clippy::too_many_arguments)]
fn raise_hand(
    mut commands: Commands,
    model: Option<Res<Model>>,
    fit: Option<Res<BoardFit>>,
    mut faces: Query<(Entity, &HandSlot, &Interaction, Option<&mut UiTransform>, Has<LiftedCard>)>,
    dev: Option<Res<crate::dev::Dev>>,
) {
    let (Some(model), Some(fit)) = (model, fit) else { return };
    let game = &model.0;
    let rise = (1.0 - layout::PEEK) * fit.size_of(game.side).height();
    // `NETRUNNER_LIFT`: the first card is held raised for a screenshot,
    // as if the pointer rested on it.
    let forced = dev.is_some_and(|dev| dev.lift && dev.autoplayed >= dev.autoplay);
    let menu_card = game.menu.as_ref().and_then(|menu| match &menu.target {
        Target::HandCard(card) => Some(card.clone()),
        _ => None,
    });
    let raised: Option<(usize, Vec2)> = if let Some(drag) = game.dragging.as_ref().filter(|drag| drag.dragging) {
        Some((drag.what, Vec2::new(drag.now.0 - drag.from.0, drag.now.1 - drag.from.1 - rise)))
    } else if let Some(card) = menu_card {
        game.hand.cards().iter().position(|each| *each == card).map(|slot| (slot, Vec2::new(0.0, -rise)))
    } else if game.menu.is_none() && !game.covered() {
        // A press not yet a drag keeps the card up: it was raised when
        // the press found it, and dropping it then would move it out from
        // under the pointer that is about to pull it.
        let held = game.dragging.as_ref().map(|drag| drag.what);
        faces
            .iter()
            .find(|(_, slot, interaction, _, _)| Some(slot.0) == held || matches!(interaction, Interaction::Hovered | Interaction::Pressed) || (forced && slot.0 == 0))
            .map(|(_, slot, _, _, _)| (slot.0, Vec2::new(0.0, -rise)))
    } else {
        None
    };
    for (entity, slot, _, transform, lifted) in &mut faces {
        match raised {
            Some((at, offset)) if at == slot.0 => {
                let translation = Val2::px(offset.x, offset.y);
                match transform {
                    Some(mut transform) if transform.translation != translation => transform.translation = translation,
                    Some(_) => {}
                    None => {
                        commands.entity(entity).insert(UiTransform { translation, ..UiTransform::IDENTITY });
                    }
                }
                if !lifted {
                    commands.entity(entity).insert((LiftedCard, OverrideClip, GlobalZIndex(12)));
                }
            }
            _ if lifted => {
                commands.entity(entity).remove::<(LiftedCard, OverrideClip, GlobalZIndex)>();
                if let Some(mut transform) = transform {
                    *transform = UiTransform::IDENTITY;
                }
            }
            _ => {}
        }
    }
}

// ---- the control bar and the rail ----

/// One button per control of the person's side, in the bar's fixed
/// order; enabled when the engine lists what it means and the person
/// may act, greyed otherwise. Greyed rather than absent so "End turn"
/// is always in the same place.
fn spawn_control_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
    // A replay's bar moves through the record instead: the same row, so
    // the board keeps its layout, and nothing on it is an action.
    if let Some(at) = &game.replay {
        for step in crate::models::replay::Step::BAR {
            if step.moves(at.cursor, at.len) {
                parent.spawn(widgets::button(theme, step.label(), Val::Auto, ReplayClick(step)));
            } else {
                parent.spawn(widgets::disabled_button(theme, step.label(), Val::Auto, ReplayClick(step)));
            }
        }
        return;
    }
    for control in Control::for_side(game.side) {
        let offered = game.awaiting && game.actions.for_control(*control).is_some();
        // Continue names the step it takes the game to, so its words change
        // with every step; its width does not, or the centred bar would
        // shift under the pointer between two presses of it.
        let (label, width) = match control {
            Control::Continue if offered => (game.actions.continue_label().to_string(), px(layout::CONTINUE_WIDTH)),
            Control::Continue => (control.label().to_string(), px(layout::CONTINUE_WIDTH)),
            _ => (control.label().to_string(), Val::Auto),
        };
        if offered {
            // Continue is the bar's one filled pill: the move the game
            // most expects, at the place the hand always finds it.
            let kind = if *control == Control::Continue { widgets::ButtonKind::Primary } else { widgets::ButtonKind::Secondary };
            parent.spawn(widgets::styled_button(theme, kind, label, width, Click::Control(*control)));
        } else {
            parent.spawn(widgets::disabled_button(theme, label, width, Click::Control(*control)));
        }
    }
}

/// The prompt, the decisions it is asking, and — with the play helper on
/// — every legal action in the engine's order.
fn spawn_rail(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, helper: bool) {
    if let Some(at) = &game.replay {
        parent.spawn((widgets::label(theme, format!("Replay · step {} of {}", at.cursor, at.len)), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        parent.spawn((widgets::dim(theme, at.title.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(lesson) = &game.lesson
        && !game.finished()
    {
        spawn_coaching(parent, theme, lesson, game);
    }
    if let Some(prompt) = &game.prompt {
        parent.spawn((widgets::label(theme, prompt.title.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        if !prompt.detail.is_empty() {
            parent.spawn((widgets::dim(theme, prompt.detail.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
    }
    if let Some(rejection) = &game.rejection {
        parent.spawn((widgets::notice(theme, format!("Rejected: {rejection}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(reason) = &game.break_stopped {
        parent.spawn((widgets::notice(theme, reason.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
    if let Some(reason) = &game.stalled {
        parent.spawn((widgets::notice(theme, reason.clone(), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        return;
    }
    if game.over.is_some() {
        parent.spawn(widgets::dim(theme, "The match is over."));
        return;
    }
    // The state of the encounter is the run panel's (`fill_encounter`),
    // directly above: the routes under the prompt are what change it.
    if game.replay.is_some() {
        parent.spawn((
            widgets::dim(theme, "Left and Right step, Page Up and Down ten at a time, Home and End go to either end, S is the other chair. A click reads a card."),
            TextLayout::new(Justify::Left, LineBreak::WordBoundary),
        ));
        return;
    }
    // The Corp's "no more this run", and the way back out of it. On the
    // rail, not the bar, for Take it back's reason below; and above the
    // "thinking" line, because while it is on the person is mostly not
    // being asked anything, and that is when they may want to stop it.
    if let Some(label) = game.run_pass_label() {
        parent.spawn(widgets::button(theme, label, percent(100), Click::PassTheRun));
    }
    if !game.awaiting {
        parent.spawn(widgets::dim(theme, if game.view.is_some() { "Opponent is thinking…" } else { "Setting up…" }));
        return;
    }
    // The decisions are the pop-up's (`spawn_decision_popup`), not the
    // rail's; the rail keeps the prompt's words and, when on, the flat
    // panel. An encounter's routes are the exception: they sit under the
    // prompt, because an encounter has no pop-up and one on every piece
    // of ICE would cover the ICE it is about. The number keys press them
    // as they press the pop-up's.
    let view = game.view.as_ref();
    for (index, route) in game.breaks.iter().enumerate() {
        let label = view.map_or_else(|| route.price(), |view| route.label(view, game.registry()));
        let mut button = parent.spawn(widgets::button(theme, label, percent(100), Click::Break(index)));
        button.entry::<Node>().and_modify(|mut node| node.padding = UiRect::axes(px(10), px(6)));
    }
    // Beside the routes and not on the control bar: the bar is the basic
    // actions, greyed when the engine does not list them, and this is
    // not an action.
    if let Some(label) = game.back_label() {
        parent.spawn(widgets::button(theme, label, percent(100), Click::TakeBack));
    }
    if !helper {
        if game.prompt.is_none() {
            parent.spawn((widgets::dim(theme, "Your turn: click a card or a zone for what it can do, right-click to read it, or use the bar. ? lists the keys."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
        }
        return;
    }
    parent.spawn(widgets::label(theme, "Every action"));
    let list = parent
        .spawn((
            ActionList,
            bevy::ui_widgets::ScrollArea,
            Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column, row_gap: px(4), overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|list| {
            for index in 0..game.actions.entries.len() {
                entry_button(list, theme, game, index, percent(100));
            }
        })
        .id();
    parent.spawn((Node { width: percent(100), flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(list).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, list));
    });
}

/// A button for entry `index`, its label centred like every pill's, at
/// `width`.
///
/// **The width is the caller's** because two of the three callers are
/// inside a wrapping container, where a percentage has nothing to
/// resolve against while the container is measured: the label came out
/// measured a word to a line and every row kept that height, which ran
/// two rows of cards off the window. The rail's list is the one caller
/// still in ordinary flow, and it passes `percent(100)`.
fn entry_button(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, index: usize, width: Val) -> Option<Entity> {
    let entry = game.actions.entries.get(index)?;
    let mut button = parent.spawn(widgets::button(theme, entry.label.clone(), width, Click::Entry(index)));
    button.entry::<Node>().and_modify(|mut node| node.padding = UiRect::axes(px(10), px(6)));
    Some(button.id())
}

/// The menu a click opened: the target's name and one button per
/// entry, or a line saying so when there is nothing,
/// in a small panel just above the box the target was laid out in and
/// centred on it — so it is in one place for a card however the card
/// was clicked, and the card stays in view beneath it. When the box is
/// too near the top for the menu to fit above, it sits just below
/// instead (a header along the top edge, from the Runner's chair). The
/// panel takes `Interaction` and blocks, so a click on its ground is a
/// click on the menu, not on the card beneath. Between the decision
/// pop-up and the overlays in depth: a sheet covers it, it covers the
/// pop-up.
///
/// **It is pinned by the edge that faces the card and grows away from
/// it** ([`layout::menu_box`]), so its height is never guessed and it
/// cannot leave the window. What this replaced computed a `top` from an
/// estimate — a 40px row per entry — and a menu whose labels wrapped was
/// half as tall again, so its last options went off the bottom of the
/// window, where the screen root's `Overflow::clip` ate them. Measuring
/// the panel once the layout had run was the other way and was rejected:
/// `ComputedNode` is written in `PostUpdate`, so a measured placement is
/// a frame late by construction — a flicker on the most-used click on
/// the board — and the headless tests have no layout pass to measure it
/// with.
///
/// **The panel is itself the wrapping container**, so when the room on
/// its side runs out the entries flow into a second column rather than
/// being clipped: every option stays on the window, which is what
/// opening a menu promised. A scrolling list was asked about and turned
/// down — a scroll bar makes an option reachable, not visible. An inner
/// entries container would not do, because *its* auto width is measured
/// under a max-content constraint, where taffy never wraps; the panel is
/// `position: Absolute` and so is measured under a definite space, where
/// it does. Hence also the `px` width on every child: a percentage has
/// nothing to resolve against there. With two columns the heading sits
/// atop the first rather than spanning both, which reads as a title and
/// costs nothing.
fn spawn_actions_menu(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game, menu: &crate::models::game::Menu, window: Vec2) {
    let place = layout::menu_box((window.x, window.y), menu.over, menu.entries.len());
    let accent = theme.accent;
    let mut panel = parent.spawn((ActionsMenu, MenuPart, Interaction::None, FocusPolicy::Block, GlobalZIndex(15), widgets::panel(theme, Val::Auto)));
    // Its own slot, replacing the one `widgets::panel` supplied, so a
    // skin can paint the menu apart from the sheet. The `and_modify`
    // below stays: `dress` only runs on `Added<Dressed>`, a frame after
    // the commands flush, so leaving the accent to it would show the
    // panel's own border colour for that frame.
    panel.insert((widgets::Dressed::still(Slot::PanelMenu, Drawn::new(theme.glass_strong, accent)), BackgroundColor(theme.glass_strong)));
    panel.entry::<Node>().and_modify(move |mut node| {
        node.position_type = PositionType::Absolute;
        // Wrapping is always on and only ever bites when the room runs
        // out, so the one-column menu every screenshot shows is the same
        // code path as the two-column one.
        node.flex_wrap = FlexWrap::Wrap;
        node.column_gap = px(layout::ROW_GAP);
        node.align_content = AlignContent::FlexStart;
        node.padding = UiRect::all(px(layout::MENU_PADDING));
        place_node(&mut node, place);
    });
    panel.entry::<BorderColor>().and_modify(move |mut border| *border = BorderColor::all(accent));
    panel.with_children(|panel| {
        let row = px(layout::MENU_ENTRY);
        panel.spawn((widgets::label(theme, target_title(game, &menu.target)), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { width: row, ..default() }));
        if menu.entries.is_empty() {
            let line = if game.awaiting { "Nothing to do here right now." } else { "Not your decision right now." };
            panel.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { width: row, ..default() }));
        }
        for index in &menu.entries {
            if let Some(button) = entry_button(panel, theme, game, *index, row) {
                panel.commands().entity(button).insert(MenuPart);
            }
        }
    });
}

/// Writes a [`layout::MenuBox`] onto a node: one horizontal inset and one
/// vertical, the other of each left `Val::Auto` so the panel grows away
/// from the edge it was pinned to, and the room that side has as its
/// maximum.
fn place_node(node: &mut Node, place: layout::MenuBox) {
    node.left = place.left.map_or(Val::Auto, px);
    node.right = place.right.map_or(Val::Auto, px);
    node.top = place.top.map_or(Val::Auto, px);
    node.bottom = place.bottom.map_or(Val::Auto, px);
    node.max_width = px(place.max_width);
    node.max_height = px(place.max_height);
}

/// Keeps an open menu on the window and on its card.
///
/// It is placed from two things that can both move under it — the
/// window, which a resize changes, and the box the target was laid out
/// in, which any board redraw changes — while the menu itself is spawned
/// only by a *rail* redraw. Without this, a resize left the menu where
/// the old window had put it. Re-anchoring rather than respawning,
/// because a respawn would take the hover and the pressed state of the
/// button under the pointer with it.
///
/// A target whose `ComputedNode` is empty is skipped: it has not been
/// laid out yet — the frame after a redraw, and every frame in the
/// headless tests, which run without a `UiPlugin` — and its zero-size
/// box would drag the menu into the window's corner.
///
/// **A target can have more than one box** — a Trojan is a chip on its
/// ice and a ghost in the program row, and until 24 September 2026 an
/// ICE was also a chip on the run lane — so the box re-anchored to is
/// the one nearest the box the click came from, never the first the
/// query happens to yield: that was the Corp's rez menu opening over the
/// lane, and hopping onto the ICE only once the pointer moved.
fn place_menu(fit: Option<Res<BoardFit>>, model: Option<Res<Model>>, targets: Query<(&Click, &ComputedNode, &UiGlobalTransform)>, mut panel: Query<&mut Node, With<ActionsMenu>>) {
    let (Some(fit), Some(model)) = (fit, model) else { return };
    let Ok(mut node) = panel.single_mut() else { return };
    let Some(menu) = &model.0.menu else { return };
    let clicked = Vec2::new(menu.over.x, menu.over.y);
    let over = targets
        .iter()
        .filter(|(click, computed, _)| matches!(click, Click::Target(target) if *target == menu.target) && !computed.is_empty())
        .map(|(_, computed, transform)| anchor_of(computed, transform))
        .min_by(|a, b| Vec2::new(a.x, a.y).distance_squared(clicked).total_cmp(&Vec2::new(b.x, b.y).distance_squared(clicked)))
        .unwrap_or(menu.over);
    let place = layout::menu_box((fit.window.x, fit.window.y), over, menu.entries.len());
    let mut fresh = node.clone();
    place_node(&mut fresh, place);
    if fresh != *node {
        *node = fresh;
    }
}

/// The narrowest the decision pop-up's panel is drawn, and what its own
/// padding and border take off that on each side.
const POPUP_MIN_WIDTH: f32 = 520.0;

/// The least width of a decision pop-up's pill, so a row of short answers
/// ("Keep hand", "Mulligan") is a row of like buttons, not two words.
const POPUP_BUTTON_MIN: f32 = 200.0;
const POPUP_PADDING: f32 = 17.0;

/// The decision pop-up: the prompt's words as its heading and one
/// button per decision, centred over the board. The container is the
/// whole window so the panel can be centred in it, and ignores picking
/// so the board beneath stays clickable; only the panel and its
/// buttons are hit. Under the overlays (`GlobalZIndex(20)`), so a
/// sheet opened to read a card covers it.
///
/// **At an access the card itself is in the panel, above the heading**
/// (`netrunner_client::access`), which makes this the second thing on
/// the board to carry a card and actions together — the score area
/// being the first. §4g's rule that a reading surface offers nothing
/// holds everywhere it can: a card on the table has a tile, so its
/// actions belong on the menu that tile's click opens, and its sheet
/// stays read-only. An accessed card has no tile. It is in HQ or R&D,
/// face down, and this panel is the only place it exists, so the
/// alternative to putting its actions here is a person reading a name
/// and guessing. The face is `FaceSize::Large`, the same as a sheet's,
/// and fits the 520px panel with room to spare.
fn spawn_decision_popup(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, decisions: &[usize], window: Vec2) {
    let view = game.view.as_ref();
    // The access owns the panel's words when there is one: its title
    // names the card, and its facts are the live costs (a grid may have
    // raised the printed trash cost) rather than `Prompt`'s single line.
    let access = view.and_then(|view| Access::of(view, &core.registry));
    // At the mulligan, the start-of-game box: both identities and the
    // opening hand turned up, over the keep and the mulligan
    // (`netrunner_client::board::opening`, Phase 7 §8 item 16).
    let opening = view.and_then(|view| Opening::of(view, &game.tally));
    let (title, detail) = match (&access, &game.prompt) {
        (Some(access), _) => (access.title(), access.facts().join("\n")),
        (None, Some(prompt)) if opening.is_some() => (prompt.title.clone(), opening.as_ref().map(|o| o.detail.clone()).unwrap_or_default()),
        (None, Some(prompt)) => (prompt.title.clone(), prompt.detail.clone()),
        (None, None) => ("Your decision".to_string(), String::new()),
    };
    // A card selection's candidates are drawn as their cards, each with
    // its own button under it; the rest (the confirm, "Choose none")
    // stay a list. The cards keep their positions' order, so the one just
    // selected does not jump to the end of the row.
    let selection = game.actions.selection();
    let position_of = |index: usize| match &game.actions.entries[index].action {
        PlayerAction::ToggleCardSelection { position } if selection.is_some_and(|s| s.candidate(*position).is_some()) => Some(*position),
        _ => None,
    };
    let mut choices: Vec<(usize, usize)> = decisions.iter().filter_map(|index| position_of(*index).map(|position| (position, *index))).collect();
    choices.sort();
    let buttons: Vec<usize> = decisions.iter().copied().filter(|index| position_of(*index).is_none()).collect();
    // Otherwise the one card the prompt is about: the accessed card, the
    // card going in, or the card whose text is asking.
    let single: Option<CardId> = match &access {
        Some(access) => Some(access.card.clone()),
        None if choices.is_empty() && opening.is_none() => view.and_then(|view| Prompt::card(view, &core.registry)),
        None => None,
    };
    // What the words and the list of buttons take, guessed before the
    // layout has run: the panel's padding, the heading and the lines
    // under it, a button per row with the gap between each, and the
    // caption under a lone card. The cards get the rest.
    //
    // **A guess, and no longer load-bearing.** It decides only how big
    // the cards are *drawn*; what keeps every button on the window is
    // the cap on the panel below and the card row being the only thing
    // that gives. It used to count one line per line of text, so a
    // heading or a button label that wrapped was a row of the panel
    // nobody had budgeted, and the buttons under it went off the bottom
    // — the same fault the actions menu had, in the one place a pop-up
    // can have it. The width assumed is the narrowest the panel can be,
    // so a panel that comes out wider has fewer lines than this, never
    // more.
    let remember = game.optional_prompt();
    let inner = POPUP_MIN_WIDTH - 2.0 * POPUP_PADDING;
    let lines = |text: &str, size: f32| layout::wrapped_lines(text, inner, size) as f32;
    let label_rows: f32 = buttons.iter().filter_map(|index| game.actions.entries.get(*index)).map(|entry| lines(&entry.label, size::BODY) * 22.0 + 14.0 + layout::ROW_GAP).sum();
    let chrome = 2.0 * 16.0
        + lines(&title, size::HEADING) * 30.0
        + 8.0
        + if detail.is_empty() { 0.0 } else { lines(&detail, size::SMALL) * 22.0 + 8.0 }
        + game.rejection.as_ref().map_or(0.0, |_| 30.0)
        + label_rows
        + game.back_label().map_or(0.0, |_| 22.0 + 14.0 + layout::ROW_GAP)
        + remember.as_ref().map_or(0.0, |_| lines(REMEMBER_CAPTION, size::SMALL) * 22.0 + 22.0 + 14.0 + 2.0 * layout::ROW_GAP)
        + if choices.is_empty() && opening.is_none() { layout::CHOICE_CAPTION } else { 0.0 };
    let available = (window.x - 2.0 * layout::PADDING - 2.0 * POPUP_PADDING, window.y - 2.0 * layout::PADDING - chrome);
    let count = if choices.is_empty() { usize::from(single.is_some()) } else { choices.len() };
    let (face, per_row) = match &opening {
        Some(opening) => layout::opening_faces(available, opening.hand.len()),
        None => layout::choice_faces(available, count),
    };
    let size = if face >= FaceSize::Large.width() { FaceSize::Large } else { FaceSize::Board(face as u16) };
    let width = if let Some(opening) = &opening {
        let across = per_row.min(opening.hand.len()).max(2) as f32;
        POPUP_MIN_WIDTH.max(across * size.width() + (across - 1.0) * layout::CHOICE_GAP + 2.0 * POPUP_PADDING)
    } else if choices.is_empty() { POPUP_MIN_WIDTH.max(size.width() + 2.0 * POPUP_PADDING) } else {
        let row = per_row as f32 * size.width() + (per_row as f32 - 1.0) * layout::CHOICE_GAP;
        POPUP_MIN_WIDTH.max(row + 2.0 * POPUP_PADDING)
    };
    parent
        .spawn((
            DecisionPopup,
            GlobalZIndex(10),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|screen| {
            let accent = theme.accent;
            let mut panel = screen.spawn(widgets::panel(theme, px(width)));
            // As the actions menu does: its own slot over the one the
            // panel supplied, and the immediate recolour kept, because
            // `dress` lands a frame later.
            panel.insert((widgets::Dressed::still(Slot::PanelDecision, Drawn::new(theme.glass_strong, accent)), BackgroundColor(theme.glass_strong)));
            panel.entry::<BorderColor>().and_modify(move |mut border| *border = BorderColor::all(accent));
            // **Capped at the window, and the cards are what give.** The
            // panel is centred, so a cap is all it takes for it to be on
            // the window; the shrinking is then shared out by the rule
            // `choice_faces` already states in words — every text node
            // and every button is rigid (`widgets::button` already sets
            // `flex_shrink: 0.0`), and the card row alone may lose
            // height. A card that is drawn short is still a card, and a
            // secondary click reads it full size; a button off the
            // bottom is an action the person cannot take.
            panel.entry::<Node>().and_modify(move |mut node| node.max_height = px(window.y - 2.0 * layout::PADDING));
            panel.with_children(|panel| {
                // `min_height: px(0)` with the clip is what lets it: a
                // flex item's automatic minimum is its content, so
                // without both the row would refuse to give and push the
                // buttons out again. A clip is not a scroll container —
                // nothing on the board is one.
                let gives = Node { width: percent(100), flex_shrink: 1.0, min_height: px(0), overflow: Overflow::clip(), ..default() };
                let rigid = Node { flex_shrink: 0.0, ..default() };
                if let Some(card) = &single {
                    panel.spawn(Node { justify_content: JustifyContent::Center, ..gives.clone() }).with_children(|row| {
                        spawn_choice_card(row, theme, core, images, Some(card), game.side.other(), size, ());
                    });
                }
                if let Some(opening) = &opening {
                    let cards = || Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        column_gap: px(layout::CHOICE_GAP),
                        row_gap: px(layout::CHOICE_GAP),
                        width: percent(100),
                        ..default()
                    };
                    // One row of the panel gives, as for any pop-up: the
                    // identities and the hand are one block of cards, so a
                    // short window takes height off the block, never off
                    // one of the two by a split flexbox would choose.
                    panel.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(layout::CHOICE_GAP), ..gives.clone() }).with_children(|panel| {
                        // Whose each identity is, under it; the person's own
                        // first, as their side of the table is nearer.
                        panel.spawn((OpeningIdentities, cards())).with_children(|row| {
                            for (card, side, words) in [
                                (&opening.identity, opening.side, format!("You · {:?}", opening.side)),
                                (&opening.opponent_identity, opening.side.other(), format!("{:?}", opening.side.other())),
                            ] {
                                row.spawn(Node { width: px(size.width()), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(2), ..default() }).with_children(|cell| {
                                    spawn_choice_card(cell, theme, core, images, card.as_ref(), side, size, ());
                                    cell.spawn(widgets::dim(theme, words));
                                });
                            }
                        });
                        // The hand, whole: the one moment in the game every
                        // card in it matters, drawn nowhere else but as the
                        // board's peek. Read, never pressed — a secondary
                        // click opens a card, as on the board.
                        panel.spawn(cards()).with_children(|row| {
                            for card in &opening.hand {
                                spawn_choice_card(row, theme, core, images, Some(card), opening.side, size, ());
                            }
                        });
                    });
                }
                panel.spawn((widgets::heading(theme, title), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                if !detail.is_empty() {
                    panel.spawn((widgets::dim(theme, detail), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                }
                if let Some(rejection) = &game.rejection {
                    panel.spawn((widgets::notice(theme, format!("Rejected: {rejection}"), ()), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                }
                if let Some(selection) = selection.filter(|_| !choices.is_empty()) {
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            justify_content: JustifyContent::Center,
                            column_gap: px(layout::CHOICE_GAP),
                            row_gap: px(layout::CHOICE_GAP),
                            ..gives.clone()
                        })
                        .with_children(|row| {
                            for (position, index) in &choices {
                                let Some(candidate) = selection.candidate(*position) else { continue };
                                row.spawn(Node { width: px(size.width()), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }).with_children(|cell| {
                                    // The card is the button, as its label
                                    // under it is: the pop-up's own choice,
                                    // so a press submits it.
                                    let face = spawn_choice_card(cell, theme, core, images, candidate.card.as_ref(), game.side.other(), size, (Button, Click::Entry(*index)));
                                    if candidate.selected {
                                        cell.commands().entity(face).insert(Outline::new(px(3), px(2), theme.accent));
                                    } else {
                                        glow(&mut cell.commands(), face, theme, game.actions.affordance_of_entry(*index));
                                    }
                                    let copies = selection.copies(*position);
                                    if copies > 1 {
                                        cell.spawn(widgets::dim(theme, format!("{copies} copies")));
                                    }
                                    // A button the card's width in pixels,
                                    // not the cell's 100% — the reason is on
                                    // `entry_button`'s `width`, which this
                                    // case is why it takes.
                                    entry_button(cell, theme, game, *index, px(size.width()));
                                });
                            }
                        });
                }
                // The decisions are one centred row of pills, each as wide
                // as its words (at least `POPUP_BUTTON_MIN`, at most the
                // pill's cap), wrapping when the row is full: stacked at
                // the panel's width, a wide pop-up's Keep and Mulligan were
                // two bars across the window. `Val::Auto`, never a
                // percentage, for `entry_button`'s reason.
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        justify_content: JustifyContent::Center,
                        column_gap: px(layout::ROW_GAP),
                        row_gap: px(layout::ROW_GAP),
                        width: percent(100),
                        ..rigid.clone()
                    })
                    .with_children(|row| {
                        for index in &buttons {
                            // The pop-up's own buttons glow like the cards do,
                            // and for the same reason: a decision parked on
                            // the person is the clearest case of a moment
                            // that will pass.
                            if let Some(entity) = entry_button(row, theme, game, *index, Val::Auto) {
                                row.commands().entity(entity).entry::<Node>().and_modify(|mut node| node.min_width = px(POPUP_BUTTON_MIN));
                                glow(&mut row.commands(), entity, theme, game.actions.affordance_of_entry(*index));
                            }
                        }
                    });
                // A card's "you may" can be answered for good: under the
                // answers themselves, as a second, smaller question about
                // them, and never numbered — each is one of the buttons
                // above, pressed and remembered
                // (`netrunner_client::standing`).
                if let Some(prompt) = &remember {
                    panel.spawn((widgets::dim(theme, REMEMBER_CAPTION), TextLayout::new(Justify::Left, LineBreak::WordBoundary), rigid.clone()));
                    panel.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(layout::ROW_GAP), width: percent(100), ..rigid.clone() }).with_children(|row| {
                        for answer in prompt.offered() {
                            row.spawn(widgets::button(theme, answer.label(), Val::Auto, Click::Remember(answer))).entry::<Node>().and_modify(|mut node| node.flex_grow = 1.0);
                        }
                    });
                }
                // The pop-up's wash blocks the rail, so the way back out
                // of a prompt has to be in the prompt. Last, unlit, and
                // never one of the numbered decisions: it answers nothing.
                if let Some(label) = game.back_label() {
                    panel.spawn(widgets::button(theme, label, Val::Auto, Click::TakeBack));
                }
            });
        });
}

/// The words over the pop-up's Always and Never.
const REMEMBER_CAPTION: &str = "Answer this card's question the same way every time:";

/// A card the pop-up shows: its face at `size`, marked as a
/// [`ChoiceCard`] so a secondary click reads it in the card sheet, or the
/// `concealed` side's back for a card the view does not name.
#[allow(clippy::too_many_arguments)]
fn spawn_choice_card(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, card: Option<&CardId>, concealed: Side, size: FaceSize, marker: impl Bundle) -> Entity {
    match card.and_then(|id| core.registry.get(id).map(|def| (id, def))) {
        Some((id, def)) => {
            let exact = def.numeric_id.and_then(|code| images.face(code, size));
            // Its own width not decoded yet: the sharpest copy the board
            // already has, stretched, and still asking for its own width,
            // which `poll_decoded` puts in place when it lands.
            let stand_in = if exact.is_none() { def.numeric_id.and_then(|code| images.nearest_face(code).map(|handle| (code, handle))) } else { None };
            let image = exact.or_else(|| stand_in.as_ref().map(|(_, handle)| handle.clone()));
            let entity = spawn_face(parent, theme, &Face::of(def), size, image, marker);
            // A face with no `Button` of its own still needs to know it is
            // hovered, for the secondary click.
            parent.commands().entity(entity).insert((ChoiceCard(id.clone()), Interaction::None));
            if let Some((code, _)) = stand_in {
                parent.commands().entity(entity).insert(WantsImage { code, size });
            }
            entity
        }
        None => spawn_back(parent, theme, images.back(concealed), concealed, size, marker),
    }
}

// ---- the phase bar ----

/// The turn's steps, and a run's, from `board::phase`, as a panel at the
/// foot of the right column, under the log: a column per segment — its title, then its
/// steps top to bottom — the one in play in the accent colour, the ones
/// behind it dim, and the window's line under the lot.
///
/// **A panel of the right column, not a row of the board.** It was a bar
/// under the person's hand, which kept the hand off the window's bottom
/// edge and cost every card its height; the column beside the board had
/// room it was not using. Turned off, it is hidden rather than despawned,
/// and the cards do not move either way. **It sits at the foot, not the
/// head:** its height changes with the phase, and at the head it moved
/// the run panel under it — the Runner's art bounced on every step.
fn fill_phase_bar(parent: &mut ChildSpawnerCommands, theme: &Theme, game: &Game) {
    use netrunner_client::board::phase::{self, State};
    let panel = Node {
        width: percent(100),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(10)),
        row_gap: px(6),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(8)),
        ..default()
    };
    // The panel is a button: a press opens the whole timing chart
    // (`board::timing`), the rules' steps with the one in play lit, which
    // is what the bar's handful of steps stands for.
    parent
        .spawn((
            panel,
            Button,
            Click::Timing,
            BackgroundColor(theme.panel),
            BorderColor::all(theme.panel_border),
            widgets::Dressed::still(Slot::PanelPhase, Drawn::new(theme.panel, theme.panel_border)),
        ))
        .with_children(|panel| {
            let Some(view) = &game.view else {
                panel.spawn(widgets::dim(theme, "Setting up…"));
                return;
            };
            let bar = phase::bar(view);
            panel.spawn((Node { flex_direction: FlexDirection::Row, align_items: AlignItems::FlexStart, column_gap: px(10), ..default() },)).with_children(|row| {
                for segment in &bar.segments {
                    row.spawn((Node { flex_direction: FlexDirection::Column, align_items: AlignItems::FlexStart, flex_grow: 1.0, flex_basis: px(0), min_width: px(0), row_gap: px(4), ..default() },)).with_children(|column| {
                        column.spawn((Text::new(segment.title.clone()), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        for step in &segment.steps {
                            let (text, background, border, slot) = match step.state {
                                State::Past => (theme.text_dim, theme.panel, theme.panel_border, Slot::PhaseChipPast),
                                State::Now => (theme.background, theme.accent, theme.accent, Slot::PhaseChipNow),
                                State::Ahead => (theme.text_dim, theme.background, theme.panel_border, Slot::PhaseChipAhead),
                            };
                            column.spawn((
                                PhaseStep(step.state),
                                Node {
                                    padding: UiRect::axes(px(8), px(3)),
                                    border: UiRect::all(px(1)),
                                    border_radius: BorderRadius::all(px(10)),
                                    max_width: percent(100),
                                    ..default()
                                },
                                BackgroundColor(background),
                                BorderColor::all(border),
                                widgets::Dressed::still(slot, Drawn::new(background, border)),
                                children![(Text::new(step.label.clone()), theme.font(size::SMALL), TextColor(text), TextLayout::new(Justify::Left, LineBreak::WordBoundary))],
                            ));
                        }
                    });
                }
            });
            // A line only when a window is open: the panel is not a row of
            // the board, so its height changing moves no card.
            if let Some(note) = &bar.note {
                panel.spawn((Text::new(note.clone()), theme.font(size::SMALL), TextColor(theme.accent), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
}

/// The Runner's identity while a run is on, in the right column: the
/// person asked for the Runner to appear there "hacking into" the server,
/// so a run reads at a glance from across the room.
///
/// **Its picture is the top of the scan** — the name banner and the art,
/// cut where the text box begins (`layout::IDENTITY_ART`) — as an
/// `ImageNode::rect` over the decoded copy's own pixel size, read from
/// `Assets<Image>`, so whichever resampled copy is cached crops the same.
/// With no scan
/// cached it is the identity's name alone, large, in the Runner's colour.
/// It follows the paced trail, not the view, so it appears on the run's
/// first beat and goes when the trail ends, never ahead of the trail. It
/// is paint: no button, no action, nothing the engine offered.
///
/// **While the run encounters a piece of ICE, the ICE takes the panel**
/// ([`fill_encounter`]) and the Runner comes back when it is passed.
#[allow(clippy::too_many_arguments)]
fn fill_run_panel(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, assets: Option<&Assets<Image>>, game: &Game, window: f32, above: f32) {
    if let Some(met) = game.encounter() {
        fill_encounter(parent, theme, core, images, assets, &met, window, above);
        return;
    }
    let (Some(trail), Some(view)) = (&game.trail, &game.view) else { return };
    if trail.ended() {
        return;
    }
    let card = view.runner.identity.as_ref().and_then(|id| core.registry.get(id));
    let name = card.map_or_else(|| "The Runner".to_string(), |card| card.title.clone());
    let colour = theme.side(Side::Runner);
    let panel = Node {
        width: percent(100),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(10)),
        row_gap: px(8),
        border: UiRect::all(px(2)),
        border_radius: BorderRadius::all(px(8)),
        ..default()
    };
    parent
        .spawn((panel, BackgroundColor(theme.panel), BorderColor::all(colour), widgets::Dressed::still(Slot::PanelRun, Drawn::new(theme.panel, colour))))
        .with_children(|panel| {
            // Each line says its width in pixels. The right column sizes
            // this panel before the percentage has anything to resolve
            // against, so a line left to it was measured one word to a
            // line — and the panel kept that height, a hundred pixels of
            // nothing under a one-line name.
            let line = || Node { width: px(RUN_ART_WIDTH), ..default() };
            panel.spawn((Text::new(format!("Hacking into {}", server_name(trail.server))), theme.font(size::SMALL), TextColor(colour), line()));
            // The top of the scan at the panel's inner width: the panel's
            // own copy once it has decoded, the sharpest other until then.
            // The rect is a fraction of whichever copy's own size.
            let width = RUN_ART_WIDTH;
            let scan = card.and_then(|card| card.numeric_id).and_then(|code| images.face(code, FaceSize::Board(width as u16)).or_else(|| images.nearest_face(code)));
            let size = scan.as_ref().and_then(|scan| assets?.get(scan)).map(|image| image.size_f32());
            match scan.zip(size) {
                Some((scan, size)) => {
                    let rect = Rect::new(0.0, 0.0, size.x, size.y * layout::IDENTITY_ART);
                    let height = (width * rect.height() / rect.width()).round();
                    panel.spawn((
                        RunIdentityArt(scan.clone()),
                        ImageNode { image_mode: NodeImageMode::Stretch, rect: Some(rect), ..ImageNode::new(scan) },
                        Node { width: px(width), height: px(height), flex_shrink: 0.0, border_radius: BorderRadius::all(px(6)), ..default() },
                    ));
                    // The name is on the banner; this one is for a test and
                    // a screen reader, so it is drawn small.
                    panel.spawn((RunIdentityName, Text::new(name), theme.font(size::SMALL), TextColor(theme.text_dim), line()));
                }
                None => {
                    panel.spawn((RunIdentityName, Text::new(name), theme.font(size::HEADING), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::WordBoundary), line()));
                }
            }
        });
}

/// The ICE a run is encountering, in the run panel in place of the
/// Runner: "Encountering <server>", the art of its scan, its name, its
/// printed type line, its strength — the printed number beside it when
/// something has moved it (`Encounter::strength_line`, the sheet's words)
/// — and every subroutine marked broken, fired or still pending
/// (`Game::encounter`). An encounter is a thing a person decides *in*,
/// and a sheet is a click away and a click back, so the ICE's whole face
/// is here, directly above the routes in the rail that change it.
///
/// **It replaces the Runner rather than stacking under them**, because
/// the column is 380 pixels of header, phase, prompt, routes and log,
/// and the Runner's picture says nothing during an encounter that the
/// phase panel does not.
///
/// **Not gated on `awaiting`, so it stays up while the Corp thinks**
/// (Brân 1.0's first subroutine fires, the Corp is asked where to
/// install, and both sides want to see `[!]` against that clause while
/// the question is open). It is paint: no button and no action.
///
/// **The picture is the art, not the top of the card**: an ICE prints
/// its text box above its art (`layout::ICE_ART`), so the panel crops
/// the art alone and writes the name out. With no scan cached the name
/// is drawn large in the Corp's colour, as the Runner's is.
///
/// **The marks are the terminal client's, character for character** —
/// `[x]`, `[!]`, `[ ]` — rather than a tick glyph: the bundled Noto
/// fallback is not guaranteed to carry one, and a person moving between
/// the two clients should not have to learn the marks twice. The colour
/// is what this client adds. **The tiles are deliberately left alone:**
/// a pip per subroutine on the encountered tile was rejected in §4ac —
/// a pip would not say *which* subroutine, and these lines do.
///
/// **The art is what gives when the window is short** (`layout::encounter_art`):
/// the words and the rail's prompt under the panel take their room first,
/// and the picture is scaled whole into what is left, or left out.
#[allow(clippy::too_many_arguments)]
fn fill_encounter(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, assets: Option<&Assets<Image>>, met: &Encounter, window: f32, above: f32) {
    let card = met.card.as_ref().and_then(|id| core.registry.get(id));
    let name = card.map_or_else(|| "Ice".to_string(), |card| card.title.clone());
    let heading = format!("Encountering · {}", server_name(met.server));
    let type_line = card.and_then(|card| card.type_line.clone());
    let strength = met.strength_line(&core.registry);
    let marked: Vec<(String, Color)> = met
        .subroutines
        .iter()
        .map(|sub| {
            let (mark, colour) = match sub.status {
                SubroutineStatus::Broken => ("[x]", theme.text_dim),
                SubroutineStatus::Resolved => ("[!]", theme.danger),
                SubroutineStatus::Pending => ("[ ]", theme.text),
            };
            (format!("{mark} {}", sub.text), colour)
        })
        .collect();
    let words = [heading.as_str(), name.as_str(), strength.as_str()].into_iter().chain(type_line.as_deref()).chain(marked.iter().map(|(text, _)| text.as_str()));
    let colour = theme.side(Side::Corp);
    let panel = Node {
        width: percent(100),
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(px(10)),
        row_gap: px(6),
        border: UiRect::all(px(2)),
        border_radius: BorderRadius::all(px(8)),
        ..default()
    };
    parent
        .spawn((panel, BackgroundColor(theme.panel), BorderColor::all(colour), widgets::Dressed::still(Slot::PanelEncounter, Drawn::new(theme.panel, colour))))
        .with_children(|panel| {
            // Each line states its width, as the Runner's panel's do.
            let line = || Node { width: px(RUN_ART_WIDTH), ..default() };
            let wrap = || TextLayout::new(Justify::Left, LineBreak::WordBoundary);
            panel.spawn((Text::new(heading.clone()), theme.font(size::SMALL), TextColor(colour), line()));
            let scan = card.and_then(|card| card.numeric_id).and_then(|code| images.face(code, FaceSize::Board(RUN_ART_WIDTH as u16)).or_else(|| images.nearest_face(code)));
            let size = scan.as_ref().and_then(|scan| assets?.get(scan)).map(|image| image.size_f32());
            let [left, top, right, bottom] = layout::ICE_ART;
            let art = scan.zip(size).and_then(|(scan, size)| {
                let rect = Rect::new(size.x * left, size.y * top, size.x * right, size.y * bottom);
                let natural = (RUN_ART_WIDTH, (RUN_ART_WIDTH * rect.height() / rect.width()).round());
                layout::encounter_art(natural, window, above, words).map(|drawn| (scan, rect, drawn))
            });
            match art {
                Some((scan, rect, (width, height))) => {
                    panel.spawn((
                        RunIdentityArt(scan.clone()),
                        ImageNode { image_mode: NodeImageMode::Stretch, rect: Some(rect), ..ImageNode::new(scan) },
                        Node { width: px(width), height: px(height), flex_shrink: 0.0, align_self: AlignSelf::Center, border_radius: BorderRadius::all(px(6)), ..default() },
                    ));
                    panel.spawn((EncounterName, Text::new(name.clone()), theme.font(size::BODY), TextColor(theme.text), wrap(), line()));
                }
                None => {
                    panel.spawn((EncounterName, Text::new(name.clone()), theme.font(size::HEADING), TextColor(colour), wrap(), line()));
                }
            }
            if let Some(type_line) = type_line.clone() {
                panel.spawn((EncounterLine, Text::new(type_line), theme.font(size::SMALL), TextColor(theme.text_dim), wrap(), line()));
            }
            panel.spawn((EncounterLine, Text::new(strength.clone()), theme.font(size::SMALL), TextColor(theme.text), wrap(), line()));
            for (text, colour) in marked.iter().cloned() {
                panel.spawn((EncounterLine, Text::new(text), theme.font(size::SMALL), TextColor(colour), wrap(), line()));
            }
        });
}

/// The run panel's picture width: the right column less the panel's
/// padding and border.
const RUN_ART_WIDTH: f32 = layout::RAIL_WIDTH - 2.0 * 12.0;

/// Refills the right column's three parts that follow the match rather
/// than the prompt: the status line, the phase panel and the run panel.
/// Runs before `redraw`, and reads the board's flag rather than taking
/// it, so a new view and a beat of the trail both reach it; `trail` and
/// `side` are its own.
#[allow(clippy::too_many_arguments)]
fn side_panels(
    mut commands: Commands,
    mut dirty: ResMut<Dirty>,
    model: Option<Res<Model>>,
    mut status: Query<&mut Text, With<StatusLine>>,
    mut phase: Query<(Entity, &mut Node), (With<PhaseBarRow>, Without<RunIdentity>)>,
    run: Query<Entity, With<RunIdentity>>,
    theme: Res<Theme>,
    core: Res<ClientCore>,
    mut images: ResMut<CardImages>,
    assets: Option<Res<Assets<Image>>>,
    drawn: Query<&RunIdentityArt>,
    above_the_panel: Query<&ComputedNode, Or<(With<RailHeader>, With<PhaseBarRow>)>>,
    fit: Option<Res<BoardFit>>,
    mut measured: Local<f32>,
) {
    // What the column's fixed panels hold beside the run panel, as laid
    // out — the header over it, the phase panel at the foot: the encounter
    // panel's art is sized against it (`layout::encounter_art`). It is read
    // a frame late, so when it moves during an encounter — the phase panel
    // gaining its note line, the status wrapping — the panel is redrawn to
    // fit, and it cannot oscillate, since neither height depends on it.
    let above: f32 = above_the_panel.iter().map(|node| node.size().y * node.inverse_scale_factor()).sum();
    if (above - *measured).abs() > 1.0 {
        *measured = above;
        if model.as_ref().is_some_and(|model| model.0.encounter().is_some()) {
            dirty.side = true;
        }
    }
    let window = fit.as_ref().map_or(f32::MAX, |fit| fit.window.y);
    // While a run is on, the panel wants the scan of whichever card it
    // shows — the ICE being encountered, else the Runner — at its own
    // width: ask for it once, and redraw when it lands, since the copy
    // that stands in for it (the strip's identity, a tile's) is far
    // narrower.
    let running = model.as_ref().and_then(|model| {
        let game = &model.0;
        let id = match game.encounter() {
            Some(met) => met.card?,
            None => {
                game.trail.as_ref().filter(|trail| !trail.ended())?;
                game.view.as_ref()?.runner.identity.clone()?
            }
        };
        core.registry.get(&id)?.numeric_id
    });
    if let Some(code) = running {
        let size = FaceSize::Board(RUN_ART_WIDTH as u16);
        match images.face(code, size) {
            Some(sharp) => {
                if drawn.iter().any(|art| art.0 != sharp) {
                    dirty.side = true;
                }
            }
            None => {
                if let netrunner_card_sync::ImageStatus::Cached(path) = core.images.status(code) {
                    images.request(code, size, path);
                }
            }
        }
    }
    if !(dirty.board || dirty.trail || dirty.side) {
        return;
    }
    dirty.side = false;
    dirty.trail = false;
    let Some(model) = model else { return };
    let game = &model.0;
    for mut text in &mut status {
        text.0 = status_line(game);
    }
    let shown = core.settings.desktop.phase_bar;
    for (entity, mut node) in &mut phase {
        node.display = if shown { Display::Flex } else { Display::None };
        let mut panel = commands.entity(entity);
        panel.despawn_children();
        if shown {
            panel.with_children(|parent| fill_phase_bar(parent, &theme, game));
        }
    }
    for entity in &run {
        commands.entity(entity).despawn_children().with_children(|parent| fill_run_panel(parent, &theme, &core, &images, assets.as_deref(), game, window, above));
    }
}

// ---- the overlays ----

/// A lesson's opening or closing words: its title and its paragraphs.
fn spawn_lesson_words(panel: &mut ChildSpawnerCommands, theme: &Theme, title: &str, words: &str) {
    panel.spawn((widgets::heading(theme, title), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    for paragraph in words.split('\n').filter(|paragraph| !paragraph.is_empty()) {
        panel.spawn((widgets::label(theme, paragraph), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
    }
}

fn spawn_overlay(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, window: Vec2) {
    // The widths are `layout`'s: a card alone, an install beside its
    // state, a zone's contents.
    let card_alone = game.inspecting.is_some() || game.sheet.as_ref().is_some_and(|s| !matches!(s.target, Target::Install(_)) && game.card_of(&s.target).is_some());
    let width = if game.lesson.is_some() && (game.intro_open() || game.finished()) {
        // A lesson's opening and closing words are a paragraph or two.
        px(680)
    } else if game.finished() || game.confirm_quit || game.options_open || game.help_open {
        px(560)
    } else if game.timing_open {
        // Two columns on the Runner's turn (its turn beside the run); the
        // Corp's turn is one chart, and a wide panel strands its numbers.
        let charts = game.view.as_ref().map_or(1, |view| netrunner_client::board::timing::timing(view).charts.len());
        px(if charts > 1 { 1000 } else { 560 })
    } else if card_alone {
        px(layout::SHEET_CARD)
    } else if game.sheet.as_ref().is_some_and(|s| matches!(s.target, Target::Install(_))) {
        px(layout::SHEET_INSTALL)
    } else {
        px(layout::SHEET_ZONE)
    };
    // A surface opened only to read sits at the right, over the right
    // column, under a light wash; a form or a question stays in the
    // middle under the full one (`layout::Placement`, Phase 7 §8 item
    // 21). Placed by flex rather than by `Placement::left` against
    // `window`, so a resize moves the panel without a respawn; the two
    // put the panel's right edge at the same place.
    let placement = layout::Placement::of(game.dismissed_by_a_click_away());
    // A reading surface closes on a click that misses it, so the wash is
    // a button carrying the same `Click::CloseOverlay` the Close button
    // used to — no new system and no new intent, and because it resolves
    // to `Intent::Back` it pops one level, taking an inspected card back
    // to the zone sheet under it exactly as Escape does. A form and the
    // three panels that are asking a question stay modal
    // (`Game::dismissed_by_a_click_away`), so the wash is inert there.
    //
    // **`FocusPolicy::Block` is in the spawn bundle, unconditionally, and
    // both halves of that matter.** `Node` *requires* `FocusPolicy`, whose
    // default is `Pass` — so a wash that did not say `Block` would let
    // `ui_focus_system` walk straight past it into the board behind, and
    // the board's tiles and control-bar buttons are `Themed` buttons that
    // would take the press. A click that missed the panel could close the
    // sheet and end the turn in the same frame. It is unconditional
    // because a *form*'s wash has to hold a press too: it declines to act
    // on one, which is not the same as letting it through. Inheriting the
    // `Block` from `Button` would not have worked either — a required
    // component is inserted with `Keep`, and the spawn above has already
    // given the entity `Node`'s `Pass`, so `Button`'s `Block` is silently
    // skipped on a post-spawn `insert`. Both facts are the opposite of
    // what they look like, which is why they are written down rather
    // than left to a requirement to express.
    //
    // The `Dressed` is not decoration either. `button_feedback` resets a
    // `Themed` node's background to `theme.button` on every interaction
    // change unless it carries one — the limitation recorded in place on
    // that system — so an undressed scrim would turn button-grey after
    // the first hover. `Dressed::still` hands back the same `Drawn` for
    // all three interactions, so the wash never moves under the pointer.
    let wash = theme.wash.with_alpha(placement.wash_alpha());
    let (justify_content, padding) = match placement {
        layout::Placement::Centre => (JustifyContent::Center, UiRect::ZERO),
        layout::Placement::Side => (JustifyContent::FlexEnd, UiRect::right(px(layout::PADDING))),
    };
    let mut overlay = parent.spawn((
        Overlay,
        FocusPolicy::Block,
        GlobalZIndex(20),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            justify_content,
            align_items: AlignItems::Center,
            padding,
            ..default()
        },
        BackgroundColor(wash),
    ));
    if game.dismissed_by_a_click_away() {
        overlay.insert((Button, Interaction::None, widgets::Themed, Click::CloseOverlay, widgets::Dressed::still(Slot::OverlayScrim, Drawn::new(wash, Color::NONE))));
    }
    overlay
        .with_children(|screen| {
            // And the panel blocks for the same reason, so a press on its
            // padding, its row gaps or the empty column beside an
            // install's face is not a press on the wash behind it. No
            // `Interaction` with it: `spawn_actions_menu` carries one
            // because `board_click` reads the menu's, and nothing reads
            // this panel's.
            let mut sheet = screen.spawn((FocusPolicy::Block, widgets::panel(theme, width)));
            // Its own slot over the one `widgets::panel` supplied, so a
            // skin can dress the game's surfaces without also repainting
            // the main menu and the settings, which `Slot::Panel` reaches
            // too. Inserted rather than bundled: two `Dressed` in one
            // bundle is a duplicate-component panic.
            sheet.insert((widgets::Dressed::still(Slot::PanelSheet, Drawn::new(theme.glass_strong, theme.glass_border)), BackgroundColor(theme.glass_strong)));
            sheet.with_children(|panel| {
                if let Some(reason) = &game.stalled {
                    panel.spawn(widgets::heading(theme, "The match stopped"));
                    panel.spawn((widgets::dim(theme, reason.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    if let Some(line) = &game.saved_report {
                        panel.spawn((widgets::dim(theme, line.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        if game.saved_report_path.is_some() {
                            row.spawn(widgets::button(theme, "Watch it", Val::Auto, Click::WatchReport));
                        }
                        row.spawn(widgets::button(theme, "Menu", Val::Auto, Click::Menu));
                    });
                } else if let Some(lesson) = game.lesson.as_ref().filter(|lesson| lesson.outro.is_some()) {
                    spawn_lesson_words(panel, theme, &format!("{} — complete", lesson.title), lesson.outro.as_deref().unwrap_or_default());
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Learn to Play", Val::Auto, Click::Menu));
                        // The end of a track goes on to its starter game, as
                        // the terminal's `learn track` does.
                        if lesson.has_next {
                            row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Next lesson", Val::Auto, Click::NextLesson));
                        } else {
                            row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Play the starter game", Val::Auto, Click::StarterGame));
                        }
                    });
                } else if let (Some(over), Some(_)) = (&game.over, &game.lesson) {
                    // The match ended before the lesson did: the step was
                    // not reached, so it is the lesson again, not a result.
                    let won = over.winner == game.side;
                    panel.spawn(widgets::heading(theme, if won { "You won before the lesson finished" } else { "The lesson ended early" }));
                    panel.spawn((widgets::dim(theme, format!("{:?} wins: {}. The lesson's last steps were never reached.", over.winner, end_reason(over.reason))), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Learn to Play", Val::Auto, Click::Menu));
                        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Try again", Val::Auto, Click::RetryLesson));
                    });
                } else if let Some(over) = &game.over {
                    let won = over.winner == game.side;
                    panel.spawn((Text::new(if won { "You win" } else { "You lose" }), theme.font(size::HEADING), TextColor(if won { theme.accent } else { theme.danger })));
                    panel.spawn(widgets::dim(theme, format!("{:?} wins: {}", over.winner, end_reason(over.reason))));
                    if let Some(report) = &over.report {
                        for line in report.lines() {
                            panel.spawn((widgets::dim(theme, line), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                    }
                    if let Some(notice) = &over.notice {
                        panel.spawn(widgets::notice(theme, notice.clone(), ()));
                    }
                    end_table(panel, theme, &game.tally, game.side);
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::button(theme, "Play again", Val::Auto, Click::PlayAgain));
                        row.spawn(widgets::button(theme, "Menu", Val::Auto, Click::Menu));
                    });
                } else if game.confirm_quit && game.lesson.is_some() {
                    panel.spawn(widgets::heading(theme, "Leave the lesson?"));
                    panel.spawn((widgets::dim(theme, "Nothing is recorded. It starts again from the beginning under Learn to Play."), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::button(theme, "Leave", Val::Auto, Click::ConfirmQuit));
                        row.spawn(widgets::button(theme, "Keep going", Val::Auto, Click::CancelQuit));
                    });
                } else if let Some(intro) = game.lesson.as_ref().and_then(|lesson| lesson.intro.as_deref().map(|intro| (lesson.title.as_str(), intro))) {
                    spawn_lesson_words(panel, theme, intro.0, intro.1);
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::styled_button(theme, ButtonKind::Quiet, "Leave", Val::Auto, Click::Menu));
                        row.spawn(widgets::styled_button(theme, ButtonKind::Primary, "Begin", Val::Auto, Click::BeginLesson));
                    });
                } else if game.confirm_quit {
                    panel.spawn(widgets::heading(theme, "Leave the game?"));
                    let turn = game.view.as_ref().map_or(0, |v| v.turn);
                    let consequence = if turn >= netrunner_client::record::FORFEIT_FROM_TURN {
                        "From turn 3 on, a quit counts as a loss toward your suggested rung."
                    } else {
                        "Nothing is recorded this early in the game."
                    };
                    panel.spawn((widgets::dim(theme, consequence), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        row.spawn(widgets::button(theme, "Quit", Val::Auto, Click::ConfirmQuit));
                        row.spawn(widgets::button(theme, "Keep playing", Val::Auto, Click::CancelQuit));
                    });
                } else if game.options_open {
                    panel.spawn(widgets::heading(theme, "Game options"));
                    settings_screen::spawn_rows(panel, theme, core, &Row::GAME);
                    if let Some(line) = &game.saved_report {
                        panel.spawn((widgets::dim(theme, line.clone()), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                    }
                    panel.spawn(widgets::row(12.0)).with_children(|row| {
                        // A replay is already a saved game.
                        if game.replay.is_none() {
                            row.spawn(widgets::button(theme, "Save a bug report", Val::Auto, Click::SaveReport));
                        }
                        if game.saved_report_path.is_some() {
                            row.spawn(widgets::button(theme, "Watch it", Val::Auto, Click::WatchReport));
                        }
                        row.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
                    });
                } else if game.help_open {
                    help_sheet(panel, theme);
                } else if game.timing_open {
                    if let Some(view) = &game.view {
                        timing_sheet(panel, theme, &netrunner_client::board::timing::timing(view));
                    }
                } else if let Some(id) = &game.inspecting {
                    // A card read out of a pile: the face and its text,
                    // nothing to do with it from here.
                    card_sheet(panel, theme, core, images, id);
                } else if let Some(sheet) = &game.sheet {
                    match (&sheet.target, game.card_of(&sheet.target)) {
                        (Target::Install(id), card) => install_sheet(panel, theme, core, images, game, *id, card.as_ref()),
                        (_, Some(id)) => card_sheet(panel, theme, core, images, &id),
                        (Target::Server(server), None) if sheet.stack => stack_sheet(panel, theme, core, images, game, *server, window),
                        (_, None) => zone_sheet(panel, theme, core, images, game, &sheet.target, window),
                    }
                }
            });
        });
}

/// What each side did over the match, as a table under the result
/// (`netrunner_client::tally`, Phase 7 §8 item 16): a row per count, the
/// Corp's number and the Runner's in fixed columns, the person's own
/// side's heading lit. A row that is not about a side leaves its cell
/// empty rather than showing a zero that reads as a result.
fn end_table(panel: &mut ChildSpawnerCommands, theme: &Theme, tally: &netrunner_client::tally::Tally, you: Side) {
    const NUMBER: f32 = 96.0;
    panel.spawn((EndTable, Node { flex_direction: FlexDirection::Column, row_gap: px(2), width: percent(100), margin: UiRect::vertical(px(6)), ..default() })).with_children(|table| {
        let line = |table: &mut ChildSpawnerCommands, label: &str, cells: [(String, Color); 2], colour: Color| {
            table.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(8), ..default() }).with_children(|row| {
                row.spawn((Text::new(label), theme.font(size::SMALL), TextColor(colour), Node { flex_grow: 1.0, min_width: px(0), ..default() }));
                // Right-aligned by the cell, not the text: a text node is
                // as wide as its words, so a `Justify::Right` on one
                // with a fixed width still drew the number at the left.
                for (text, colour) in cells {
                    row.spawn(Node { width: px(NUMBER), flex_shrink: 0.0, justify_content: JustifyContent::FlexEnd, ..default() })
                        .with_children(|cell| {
                            cell.spawn((Text::new(text), theme.font(size::SMALL), TextColor(colour), TextLayout::new(Justify::Right, LineBreak::NoWrap)));
                        });
                }
            });
        };
        let heading = |side: Side| {
            let words = if side == you { format!("{side:?} (you)") } else { format!("{side:?}") };
            (words, if side == you { theme.accent } else { theme.text_dim })
        };
        line(table, "", [heading(Side::Corp), heading(Side::Runner)], theme.text_dim);
        let cell = |n: Option<u32>| (n.map_or_else(String::new, |n| n.to_string()), theme.text);
        for row in tally.rows() {
            line(table, row.label, [cell(row.corp), cell(row.runner)], theme.text_dim);
        }
    });
}

/// The list of keys, a row each: the key in a fixed column, what it does
/// beside it. From `shortcuts::LIST`, so the list and the keys cannot
/// disagree about which letters there are.
fn help_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme) {
    panel.spawn(widgets::heading(theme, "Keyboard shortcuts"));
    for (key, meaning) in shortcuts::LIST {
        panel.spawn((HelpRow, Node { flex_direction: FlexDirection::Row, column_gap: px(16), ..default() })).with_children(|row| {
            // A key never wraps, and its column is wide enough that it need
            // not: a wrapped measure left a blank line under "? or F1".
            row.spawn((Text::new(*key), theme.font(size::BODY), TextColor(theme.accent), TextLayout::new(Justify::Left, LineBreak::NoWrap), Node { width: px(120), flex_shrink: 0.0, ..default() }));
            row.spawn((Text::new(*meaning), theme.font(size::BODY), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { flex_grow: 1.0, min_width: px(0), ..default() }));
        });
    }
    panel.spawn(widgets::dim(theme, "A key does what its button does, only when the button would."));
    panel.spawn(widgets::button(theme, "Close", Val::Auto, Click::CloseOverlay));
}

/// The rules' timing charts with the steps the game is at lit
/// (`board::timing`): the turn on the left, the run, the breach and the
/// access on the right, so the chart fits a window without a scroll.
/// Each step is its letter, this project's words, its window's P, R, S
/// and the rule it stands for — never the rules' own text, which is Null
/// Signal Games' (`rules/NOTICE.md`). A reading surface: no heading over
/// the charts beyond their own titles, and no Close; Escape and a click
/// away close it.
fn timing_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, timing: &netrunner_client::board::timing::Timing) {
    use netrunner_client::board::timing::Chart;
    let chart = |column: &mut ChildSpawnerCommands, chart: &Chart| {
        let lit = chart.lit();
        column.spawn((Text::new(format!("{} · {}", chart.title, chart.cr)), theme.font(size::BODY), TextColor(if lit { theme.text } else { theme.text_dim })));
        for phase in &chart.phases {
            if !phase.title.is_empty() {
                column.spawn((Text::new(phase.title.clone()), theme.font(size::SMALL), TextColor(theme.text_dim), Node { margin: UiRect::top(px(2)), ..default() }));
            }
            for step in &phase.steps {
                let colour = if step.lit { theme.accent } else if lit { theme.text } else { theme.text_dim };
                column.spawn((TimingStep { cr: step.cr, lit: step.lit }, Node { flex_direction: FlexDirection::Row, column_gap: px(6), align_items: AlignItems::Center, ..default() })).with_children(|row| {
                    let marker = if step.lit { "▶" } else { "" };
                    row.spawn((Text::new(marker), theme.symbol_font(size::SMALL), TextColor(theme.accent), Node { width: px(14), flex_shrink: 0.0, ..default() }));
                    row.spawn((Text::new(step.label), theme.font(size::SMALL), TextColor(colour), Node { width: px(14), flex_shrink: 0.0, ..default() }));
                    row.spawn((Text::new(step.words), theme.font(size::SMALL), TextColor(colour), TextLayout::new(Justify::Left, LineBreak::WordBoundary), Node { flex_grow: 1.0, min_width: px(0), ..default() }));
                    if let Some(windows) = step.windows {
                        for (on, tag) in [(windows.paid, "P"), (windows.rez, "R"), (windows.score, "S")] {
                            if on {
                                row.spawn((
                                    Node { padding: UiRect::axes(px(4), px(0)), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(3)), flex_shrink: 0.0, ..default() },
                                    BorderColor::all(colour),
                                    children![(Text::new(tag), theme.font(size::SMALL - 3.0), TextColor(colour))],
                                ));
                            }
                        }
                    }
                    row.spawn((Text::new(step.cr), theme.font(size::SMALL - 3.0), TextColor(theme.text_dim), TextLayout::new(Justify::Right, LineBreak::NoWrap), Node { width: px(62), flex_shrink: 0.0, ..default() }));
                });
            }
        }
    };
    let (turn, rest) = timing.charts.split_first().expect("a turn is always charted");
    panel.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(24), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
        let column = || Node { flex_direction: FlexDirection::Column, row_gap: px(2), flex_grow: 1.0, flex_basis: px(0), min_width: px(0), ..default() };
        // The breach and the access go under the turn, and the run has the
        // right column to itself: it is the longest chart.
        columns.spawn(column()).with_children(|left| {
            chart(left, turn);
            for extra in rest.iter().filter(|c| c.title != "Run") {
                left.spawn(Node { height: px(10), ..default() });
                chart(left, extra);
            }
        });
        if let Some(run) = rest.iter().find(|c| c.title == "Run") {
            columns.spawn(column()).with_children(|right| chart(right, run));
        }
    });
    panel.spawn(widgets::dim(theme, timing.note));
}

/// The card large, and nothing else at all: the picture, or the text
/// layout (which carries the printed text) while no picture is cached.
/// What may be done with it is the menu's — the sheet once listed the
/// actions under the card's text, and the person asked for reading and
/// acting to be two clicks rather than one panel.
///
/// **No heading over it and no Close under it.** The card says its own
/// name in both tiers — a cached scan *is* the printed card, and the
/// text face draws the title in its own title row — so a heading was the
/// name twice on the fallback face and once too often on the other. The
/// button was a third door to a rule Escape already had; a click that
/// misses the panel is the second. Between them that is about eighty-six
/// logical pixels of chrome off a panel whose whole content is a
/// 380-wide face. The registry-miss arm keeps its line because there is
/// no face there to say anything.
fn card_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, id: &CardId) {
    let Some(def) = core.registry.get(id) else {
        panel.spawn(widgets::dim(theme, format!("{} is not in the registry", id.0)));
        return;
    };
    let image = def.numeric_id.and_then(|code| images.face(code, FaceSize::Large));
    spawn_face(panel, theme, &Face::of(def), FaceSize::Large, image, ());
}

/// An installed card: the face (or the back, for a card the viewer
/// cannot name) beside its state — `board::facts::install_facts`: where
/// it sits and in what order it is met, rezzed or not and the cost of a
/// rez, strength now and printed, each subroutine with its status in an
/// encounter, tokens, counters, trash cost, what it hosts. The state is
/// the one thing beside the card, because the printed face cannot show
/// it and a tile has no room; the printed text is on the face.
///
/// **The heading survives here for exactly one case**, which is why it
/// moved inside the match rather than going the way `card_sheet`'s did:
/// a card the viewer cannot name is drawn as a card *back*, and a back
/// says nothing, so `facts::hidden_title` is the only thing naming it.
/// With a card in hand the face prints its own title and a heading would
/// be the second copy.
fn install_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, id: InstallId, card: Option<&CardId>) {
    let Some(view) = &game.view else { return };
    let def = card.and_then(|c| core.registry.get(c));
    if def.is_none() {
        panel.spawn(widgets::heading(theme, facts::hidden_title(view, id)));
    }
    panel.spawn((Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, ..default() },)).with_children(|row| {
        match def {
            Some(def) => {
                let image = def.numeric_id.and_then(|code| images.face(code, FaceSize::Large));
                spawn_face(row, theme, &Face::of(def), FaceSize::Large, image, ());
            }
            None => {
                spawn_back(row, theme, images.back(Side::Corp), Side::Corp, FaceSize::Large, ());
            }
        }
        row.spawn((Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },)).with_children(|column| {
            column.spawn(widgets::label(theme, "State"));
            for line in facts::install_facts(view, id, &core.registry).unwrap_or_default() {
                column.spawn((InstallFact, Text::new(line), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
            }
        });
    });
}

/// A server's stack: every card in it, in the order a run meets them —
/// its ICE outermost first, then its root — whatever its column had room
/// to show (`layout::ServerWindow`). A row is the card (its back, for one
/// the viewer cannot name) beside its tile's title and its state
/// (`board::facts::install_facts`: where it sits in the order, rezzed or
/// not, strength, tokens, what it hosts), because the order and the state
/// are what a column of strips cannot say and a card's face cannot
/// either. It scrolls — a high-glacier remote is ten cards — by the wheel,
/// its bar and the keys (`scroll_stack`); a card reads large over it, and
/// Escape or a click away comes back. The same top-down order from both
/// chairs: it is a list to read down, not the table seen from a seat.
fn stack_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, server: ServerId, window: Vec2) {
    let Some(view) = &game.view else { return };
    panel.spawn(widgets::heading(theme, server_name(server)));
    let Some(contents) = view.corp.servers.iter().find(|s| s.server == server).filter(|s| !s.ice.is_empty() || !s.root.is_empty()) else {
        panel.spawn(widgets::dim(theme, "Nothing installed"));
        return;
    };
    let count = contents.ice.len() + contents.root.len();
    panel.spawn(widgets::dim(theme, format!("{count} card{} · outermost ICE first, down to the root", if count == 1 { "" } else { "s" })));
    let size = FaceSize::Board(layout::PILE_FACE as u16);
    let row = |column: &mut ChildSpawnerCommands, install: &netrunner_core::rules::PublicInstalledCard| {
        column.spawn((StackRow(install.install_id), Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, flex_shrink: 0.0, ..default() })).with_children(|row| {
            match install.card.as_ref().and_then(|id| core.registry.get(id).map(|def| (id, def))) {
                Some((id, def)) => {
                    let image = def.numeric_id.and_then(|code| images.face(code, size));
                    spawn_face(row, theme, &Face::of(def), size, image, (Button, Click::Inspect(id.clone())));
                }
                None => {
                    spawn_back(row, theme, images.back(Side::Corp), Side::Corp, size, ());
                }
            }
            row.spawn(Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() }).with_children(|facts_column| {
                let title = match &install.card {
                    Some(_) => facts::tile_title(view, install.install_id, &core.registry),
                    None => facts::hidden_title(view, install.install_id),
                };
                facts_column.spawn((Text::new(title), theme.font(size::BODY), TextColor(theme.text)));
                for line in facts::install_facts(view, install.install_id, &core.registry).unwrap_or_default() {
                    facts_column.spawn((Text::new(line), theme.font(size::SMALL), TextColor(theme.text_dim), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                }
            });
        });
    };
    let scroll = panel
        .spawn((
            bevy::ui_widgets::ScrollArea,
            StackScroll,
            Node { width: percent(100), max_height: px(layout::pile_height(window.y)), flex_direction: FlexDirection::Column, row_gap: px(10), overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|column| {
            // The engine's order is outermost first — the order a run
            // meets them — which is the order this reads down.
            for ice in &contents.ice {
                row(column, ice);
            }
            if !contents.root.is_empty() {
                column.spawn((widgets::label(theme, "Root"), Node { flex_shrink: 0.0, ..default() }));
                for card in &contents.root {
                    row(column, card);
                }
            }
        })
        .id();
    panel.spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(scroll).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, scroll));
    });
}

/// The keys a stack sheet scrolls by — ↑ and ↓ a line, Page Up and Page
/// Down a box, Home and End the ends — as the drop-down's list does. No
/// key on the board means an arrow, so these ask nothing of the
/// shortcuts, and they are read only while a stack sheet is up.
fn scroll_stack(keys: Res<ButtonInput<KeyCode>>, mut scrolls: Query<(&mut ScrollPosition, &ComputedNode), With<StackScroll>>) {
    const LINE: f32 = 60.0;
    for (mut position, node) in &mut scrolls {
        let page = node.size().y * node.inverse_scale_factor();
        let most = ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0);
        let y = position.y;
        let to = if keys.just_pressed(KeyCode::ArrowDown) {
            y + LINE
        } else if keys.just_pressed(KeyCode::ArrowUp) {
            y - LINE
        } else if keys.just_pressed(KeyCode::PageDown) {
            y + page
        } else if keys.just_pressed(KeyCode::PageUp) {
            y - page
        } else if keys.just_pressed(KeyCode::Home) {
            0.0
        } else if keys.just_pressed(KeyCode::End) {
            most
        } else {
            continue;
        };
        position.y = to.clamp(0.0, most);
    }
}

/// A zone: what is in it as far as the viewer may see — its actions are
/// the menu's. Every visible card is a button that reads it over the
/// sheet.
/// The score area is the exception, a list rather than a spread of
/// faces (`score_area_sheet`).
fn zone_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, target: &Target, window: Vec2) {
    let Some(view) = &game.view else { return };
    panel.spawn(widgets::heading(theme, target_title(game, target)));
    if let Target::Pile(Pile::Agendas(side)) = target {
        score_area_sheet(panel, theme, core, images, game, view, *side);
        return;
    }
    // What the zone holds, for the viewer.
    enum Shown {
        Card(CardId),
        Back(Side),
    }
    let (caption, shown): (String, Vec<Shown>) = match target {
        Target::Server(ServerId::Archives) => {
            let cards: Vec<Shown> = view.corp.archives.iter().map(|c| c.card.clone().map_or(Shown::Back(Side::Corp), Shown::Card)).collect();
            let facedown = view.corp.archives.iter().filter(|c| c.facedown).count();
            let caption = match (game.side, facedown) {
                (_, 0) => format!("{} cards", cards.len()),
                (Side::Corp, n) => format!("{} cards, {n} face down — the Runner has not seen those", cards.len()),
                (Side::Runner, n) => format!("{} cards, {n} face down", cards.len()),
            };
            (caption, cards)
        }
        Target::Server(ServerId::RnD) => (format!("{} cards, in an order nobody is shown", view.corp.rd_count), (0..view.corp.rd_count.min(5)).map(|_| Shown::Back(Side::Corp)).collect()),
        Target::Server(ServerId::Hq) => match &view.corp.hq_cards {
            Some(hand) => (format!("{} cards in hand", hand.len()), hand.iter().cloned().map(Shown::Card).collect()),
            None => (format!("{} cards, hidden", view.corp.hq_count), (0..view.corp.hq_count.min(8)).map(|_| Shown::Back(Side::Corp)).collect()),
        },
        Target::Server(server @ ServerId::Remote(_)) => {
            let cards: Vec<Shown> = view
                .corp
                .servers
                .iter()
                .filter(|s| s.server == *server)
                .flat_map(|s| s.ice.iter().chain(s.root.iter()))
                .map(|c| c.card.clone().map_or(Shown::Back(Side::Corp), Shown::Card))
                .collect();
            (if cards.is_empty() { "Nothing installed".to_string() } else { format!("{} cards, ICE first", cards.len()) }, cards)
        }
        Target::Pile(Pile::Stack) => (format!("{} cards, in an order nobody is shown", view.runner.stack_count), (0..view.runner.stack_count.min(5)).map(|_| Shown::Back(Side::Runner)).collect()),
        Target::Pile(Pile::Heap) => (format!("{} cards, all face up", view.runner.heap.len()), view.runner.heap.iter().cloned().map(Shown::Card).collect()),
        Target::Pile(Pile::Agendas(_)) => unreachable!("the score area returned above"),
        Target::HandCard(_) | Target::Install(_) | Target::Identity(_) | Target::Position(_) | Target::Rig | Target::Table => (String::new(), Vec::new()),
    };
    panel.spawn(widgets::dim(theme, caption));
    if !shown.is_empty() {
        // A wrapping row inside a column that scrolls: a pile of forty
        // is ten rows of faces, and the wheel reaches them all. The faces
        // are `layout::PILE_FACE`, a size up from the browser's grid,
        // because at `Thumb` a card in Archives could not be read; the
        // box is as many whole rows as the window has room for
        // (`layout::pile_height`), so the sheet — an overlay, never the
        // board — still fits the window at the larger size.
        let size = FaceSize::Board(layout::PILE_FACE as u16);
        let scroll = panel
            .spawn((
                bevy::ui_widgets::ScrollArea,
                Node { width: percent(100), max_height: px(layout::pile_height(window.y)), flex_direction: FlexDirection::Column, overflow: Overflow::scroll_y(), ..default() },
            ))
            .with_children(|column| {
                column.spawn(wrap_row()).with_children(|row| {
                    for item in shown {
                        match item {
                            Shown::Card(id) => {
                                if let Some(def) = core.registry.get(&id) {
                                    let image = def.numeric_id.and_then(|code| images.face(code, size));
                                    spawn_face(row, theme, &Face::of(def), size, image, (Button, Click::Inspect(id.clone())));
                                }
                            }
                            Shown::Back(side) => {
                                spawn_back(row, theme, images.back(side), side, size, ());
                            }
                        }
                    }
                });
            })
            .id();
        panel.spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(scroll).with_children(|row| {
            row.spawn(widgets::scrollbar(theme, scroll));
        });
    }
}

/// A side's score area as a list: a row per agenda — its face small,
/// its title and points — and a press on a row opens it in place, the
/// face large beside what it is worth, its counters, its printed text
/// and, for an agenda the viewer scored, the abilities it still has
/// (`Target::Install` by the handle it kept). A list rather than the
/// piles' spread of faces because the person asked to read down it and
/// open one, and a card read over the sheet hides the others.
#[allow(clippy::too_many_arguments)]
fn score_area_sheet(panel: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, game: &Game, view: &ClientView, side: Side) {
    let agendas = hud::score_area(view, side, &core.registry);
    let (points, to_win) = (if side == Side::Corp { view.corp.agenda_points } else { view.runner.agenda_points }, view.rules.winning_agenda_points);
    let caption = match agendas.len() {
        0 => format!("None yet · {points} of {to_win} points"),
        1 => format!("1 agenda · {points} of {to_win} points"),
        n => format!("{n} agendas · {points} of {to_win} points"),
    };
    panel.spawn(widgets::dim(theme, caption));
    if agendas.is_empty() {
        return;
    }
    let scroll = panel
        .spawn((
            bevy::ui_widgets::ScrollArea,
            Node { width: percent(100), max_height: px(560), flex_direction: FlexDirection::Column, row_gap: px(6), overflow: Overflow::scroll_y(), ..default() },
        ))
        .with_children(|list| {
            for (row, agenda) in agendas.iter().enumerate() {
                let open = game.expanded == Some(row);
                let def = core.registry.get(&agenda.card);
                let code = def.and_then(|d| d.numeric_id);
                list.spawn((
                    ScoreRow(row),
                    Button,
                    widgets::Themed,
                    Click::Expand(row),
                    Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12), padding: UiRect::all(px(6)), border_radius: BorderRadius::all(px(6)), flex_shrink: 0.0, ..default() },
                    BackgroundColor(theme.button),
                ))
                .with_children(|button| {
                    // The disclosure triangles are in Noto Sans Symbols 2 (U+25B8,
                    // U+25BE), not Noto Sans, which drew both as a box — and
                    // has no U+2212 minus either. Without the symbol font, a
                    // plus and an en dash.
                    let (marker, font) = if theme.has_symbols() {
                        (if open { "▾" } else { "▸" }, theme.symbol_font(size::BODY))
                    } else {
                        (if open { "–" } else { "+" }, theme.font(size::BODY))
                    };
                    button.spawn((Text::new(marker), font, TextColor(theme.text_dim), Node { width: px(18), ..default() }));
                    if let Some(def) = def {
                        spawn_face(button, theme, &Face::of(def), FaceSize::Board(56), code.and_then(|code| images.face(code, FaceSize::Board(56))), ());
                    }
                    button.spawn(widgets::label(theme, agenda.line()));
                });
                if !open {
                    continue;
                }
                list.spawn((ScoreDetails(row), Node { flex_direction: FlexDirection::Row, column_gap: px(16), align_items: AlignItems::FlexStart, padding: UiRect::left(px(28)), flex_shrink: 0.0, ..default() })).with_children(|details| {
                    if let Some(def) = def {
                        spawn_face(details, theme, &Face::of(def), FaceSize::Large, code.and_then(|code| images.face(code, FaceSize::Large)), ());
                    }
                    details.spawn((Node { flex_grow: 1.0, min_width: px(0), flex_direction: FlexDirection::Column, row_gap: px(8), ..default() },)).with_children(|column| {
                        for line in agenda.facts(side, &core.registry) {
                            column.spawn((Text::new(line), theme.font(size::SMALL), TextColor(theme.text), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                        if let Some(def) = def {
                            column.spawn((Text::new(Face::of(def).body_text(false)), theme.font(size::SMALL), TextColor(theme.text_dim), TextLayout::new(Justify::Left, LineBreak::WordBoundary)));
                        }
                        let entries = agenda.install.map(|id| game.entries_for(&Target::Install(id))).unwrap_or_default();
                        if !entries.is_empty() {
                            column.spawn(widgets::label(theme, "Actions"));
                            for index in entries {
                                entry_button(column, theme, game, index, percent(100));
                            }
                        }
                    });
                });
            }
        })
        .id();
    panel.spawn((Node { width: percent(100), flex_direction: FlexDirection::Row, column_gap: px(4), ..default() },)).add_child(scroll).with_children(|row| {
        row.spawn(widgets::scrollbar(theme, scroll));
    });
}

fn target_title(game: &Game, target: &Target) -> String {
    let title = |id: &CardId| game.registry().get(id).map_or_else(|| id.0.clone(), |c| c.title.clone());
    match target {
        Target::HandCard(card) => title(card),
        Target::Install(id) => game.card_at(*id).map_or_else(|| "This card".to_string(), |card| title(&card)),
        Target::Server(server) => server_name(*server),
        Target::Identity(side) => format!("{side:?} identity"),
        // The card the prompt means by that position, as the selection
        // names it; a position is never shown as a number.
        Target::Position(position) => game
            .view
            .as_ref()
            .and_then(|view| netrunner_client::selection::Selection::of(view, game.registry()))
            .and_then(|selection| selection.candidate(*position).map(|candidate| selection.display(candidate)))
            .unwrap_or_else(|| "A card".to_string()),
        Target::Pile(pile) => pile.name().to_string(),
        Target::Rig => "Rig".to_string(),
        Target::Table => "Table".to_string(),
    }
}

fn end_reason(reason: netrunner_client::play::GameEndReason) -> &'static str {
    use netrunner_client::play::GameEndReason;
    match reason {
        GameEndReason::AgendaThreshold => "enough agenda points",
        GameEndReason::Flatline => "the Runner was flatlined",
        GameEndReason::Deckout => "the Corp ran out of cards",
        GameEndReason::Surrender => "the other side surrendered",
        GameEndReason::Disconnected => "the other side disconnected",
        GameEndReason::TimedOut => "the other side ran out of time",
    }
}

/// The slot an install occupies, for a test that reads the board.
pub fn install_slot(view: &ClientView, id: InstallId) -> Option<InstallSlot> {
    view.corp.servers.iter().flat_map(|s| s.ice.iter().chain(s.root.iter())).find(|c| c.install_id == id).map(|c| c.slot)
}
