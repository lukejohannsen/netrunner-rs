//! Driving the client without a hand on it, for whoever cannot see the
//! window: a model checking its own work, or a CI job one day.
//!
//! Two environment variables, both ignored when unset:
//!
//! - `NETRUNNER_SCREEN=<AppScreen>` — boot goes to that screen instead of
//!   the menu (`cards`, `settings`, … — the variant name, any case).
//! - `NETRUNNER_SCREENSHOT=<path.png>` — once the screen has had time to
//!   lay out and draw, the window is saved there and the client exits.
//! - `NETRUNNER_DECK=<id>` — with `NETRUNNER_SCREEN=deckeditor`, the
//!   deck the editor opens: a saved deck is editable, a built-in one
//!   read-only (`NETRUNNER_DECKS_DIR` points it at a scratch directory).
//! - `NETRUNNER_GAME=corp|runner` — boot starts an unrecorded game on the
//!   default decks against the middle rung, the person in that chair,
//!   and goes to the board (unless `NETRUNNER_SCREEN` says elsewhere) —
//!   how the board is looked at without a hand on the form.
//!   `NETRUNNER_CORP_DECK=<id>` and `NETRUNNER_RUNNER_DECK=<id>` replace
//!   either default deck, so a card the default decks lack can be reached.
//! - `NETRUNNER_LESSON=<id>` — boot starts that lesson (`learn list` in
//!   the terminal names them) and goes to the board, which opens on the
//!   lesson's words; `NETRUNNER_BEGIN=1` puts them away so the coach is
//!   shot at the first decision. `NETRUNNER_AUTOPLAY` then wanders
//!   through the step's own actions, as it does through a game's.
//! - `NETRUNNER_REPLAY=<record.jsonl>` — boot opens that record on the
//!   replay board, where a bug report opens (its end, from the person's
//!   chair), or at `NETRUNNER_REPLAY_AT=<start|end|n>` actions in — how a
//!   saved game is looked at without a hand on the list.
//! - `NETRUNNER_AUTOPLAY=<n>` — on the board, the person's seat takes a
//!   legal action by itself, `n` times, cycling through the list so the
//!   game develops (installs, runs, rezzes) — how a board forty actions
//!   in is looked at without a hand on it. Never set for a person.
//! - `NETRUNNER_OPTIONS=1` — on the board, the gear menu is opened once
//!   the person's first decision has arrived, so the options window can
//!   be looked at over a real board.
//! - `NETRUNNER_DRAG=1` — on the board, once the person's decision has
//!   arrived (after any autoplay), the first hand card that has somewhere
//!   to go is picked up and held, so the lit places — and the column a new
//!   remote gets for the length of a drag — can be looked at.
//! - `NETRUNNER_KEYS=1` — on the board, the list of keys is opened once
//!   the person's decision has arrived (after any autoplay), so it can be
//!   looked at over a real board.
//! - `NETRUNNER_TIMING=1` — the same for the timing chart
//!   (`board::timing`, T or a press on the phase panel); with
//!   `NETRUNNER_HOLD_ICE=1` it is opened mid-run, where the turn's step
//!   and the run's are lit together.
//! - `NETRUNNER_MENU=1|most|top` — on the board, once the person's
//!   decision has arrived (after any autoplay), the actions menu a click
//!   on a card would open is opened, so the menu can be looked at. `1`
//!   takes the first hand card with an action (or the first card);
//!   `most` the target with the most entries, which is the tallest menu
//!   the board can show; `top` the target nearest the window's top edge
//!   that has an entry, which is the menu that must flip below its card.
//!   The last two exist because the first could not show a menu running
//!   off the window: a hand card sits on the bottom edge with a
//!   handful of entries.
//! - `NETRUNNER_WINDOW=<width>x<height>` — the client opens a window of
//!   that size instead of taking the whole screen. The board is
//!   fullscreen by design, so this is the only way to screenshot one
//!   cramped for room — which is what a menu or a pop-up with more in it
//!   than the window holds needs to be looked at.
//! - `NETRUNNER_LIFT=1` — on the board, once the autoplay is done, the
//!   first card in the person's hand is lifted out whole as a hover lifts
//!   it, so the lifted card over the board can be looked at.
//! - `NETRUNNER_READ=<n>` — the `n`th card a secondary click can read
//!   (`widgets::reader`) is opened once, so the centred reader can be
//!   looked at on the deck editor or over an identity picker.
//! - `NETRUNNER_IDENTITIES=1` — with `NETRUNNER_SCREEN=deckeditor` on an
//!   editable deck, Change identity is opened once, so the identity
//!   picker can be looked at.
//! - `NETRUNNER_SHEET=1` — on the board, once the person's decision has
//!   arrived (after any autoplay), the sheet a secondary click opens on
//!   the first installed Corp card on the board is opened, so an install's state can be looked at.
//! - `NETRUNNER_PILE=archives|heap|hq` — on the board, once the
//!   person's decision has arrived (after any autoplay), that zone's
//!   sheet is opened, so a pile's cards can be looked at at their size;
//!   `archives+card` (or `heap+card`, `hq+card`) also opens the first
//!   card in it large over the sheet, as a press on it does.
//! - `NETRUNNER_AGENDAS=corp|runner` — on the board, once the person's
//!   decision has arrived (after any autoplay), that side's score area
//!   is opened from its HUD readout with the first row expanded, so the
//!   list and an agenda's details can be looked at.
//! - `NETRUNNER_HOLD_RUN=1` — on the board, the pace of a run stops at
//!   its first encounter (or the server's approach, with no ice to
//!   meet) and the autoplay counts as done, so the
//!   screenshot catches a run in flight: the column under run and the
//!   run panel.
//! - `NETRUNNER_HOLD_ICE=1` — stop the autoplay at the first encounter
//!   the person is asked anything in, so the screenshot catches the
//!   marks on the ice's subroutines. **Not `HOLD_RUN` narrowed**: that
//!   one holds the *pace*, which stops the beats before the view has
//!   caught up, so the rail is still on the previous decision — and it
//!   counts an undefended server's approach as an encounter, which the
//!   sample Corps' first run very often is. This one holds the autoplay
//!   on the settled view, the way `HOLD_BREAK` does, without needing a
//!   rig that can break the whole piece.
//! - `NETRUNNER_HOLD_SELECTION=1` — on the board, the autoplay stops at
//!   the first card-selection prompt the person is asked, so the
//!   screenshot catches the pop-up's buttons naming the cards.
//! - `NETRUNNER_HOLD_INSTALL=1` — the same at the first server choice a
//!   card's text installs into (Scatter Field, Ansel 1.0), so the
//!   screenshot catches where the card may go.
//! - `NETRUNNER_HOLD_ACCESS=1` — the same at the first card the person is
//!   asked to access, so the screenshot catches the accessed card's face
//!   in the pop-up with its steal/trash/pass buttons under it.
//! - `NETRUNNER_HOLD_BREAK=1` — the same at the first encounter the
//!   person's rig can break the whole of, so the screenshot catches the
//!   routes under the prompt, each with its price.
//! - `NETRUNNER_HOLD_TROJAN=1` — the autoplay hosts a Trojan on ice
//!   whenever the engine offers it (Botulus, Tranquilizer,
//!   Chromatophores), and stops once one is hosted, so the screenshot
//!   catches the Trojan's chip on its ice and its ghost in the program
//!   row. Seat the Runner on a deck that carries one —
//!   `NETRUNNER_RUNNER_DECK=dashing_mad` — and give the autoplay room
//!   (`NETRUNNER_AUTOPLAY=300`): the wandering pick alone hosts one in
//!   some games and not others.
//! - `NETRUNNER_HOLD_MAY=1` — the autoplay stops at the first card's
//!   "you may" the pop-up offers to answer for good, so the screenshot
//!   catches the Always and Never row under the prompt. Dewi
//!   Subrotoputri asks on every successful run:
//!   `NETRUNNER_GAME=runner NETRUNNER_RUNNER_DECK=enthusiasm`.
//! - `NETRUNNER_HOLD_RUN_PASS=1|on` — the autoplay stops at the first
//!   window of a run where the rail offers the Corp "Pass for the rest of
//!   this run", so the screenshot catches the button; `on` presses it
//!   there, so the screenshot catches the run going by with the way to
//!   stop on the rail. Seat the Corp: `NETRUNNER_GAME=corp`.
//! - `NETRUNNER_DROPDOWN=<n>` — before the screenshot, the `n`th
//!   drop-down on the screen (1 is the first, counted top to bottom and
//!   left to right) is opened, so an open list can be looked at: where it
//!   opens, whether it fits the window, and its bar.
//!   `NETRUNNER_SCREEN=new-game NETRUNNER_DROPDOWN=1` is the person's own
//!   deck list at the foot of the form.
//! - `NETRUNNER_SCROLL=<x>,<y>,<lines>` — before the screenshot, the
//!   pointer is put at window position (x, y) and the wheel turned by
//!   that many lines, through the same window events winit would send;
//!   and every scrolling node's size, content size and position are
//!   logged. How "the grid does not scroll" was reproduced.
//!
//! Neither is a feature of the client; the screenshot is how a change to
//! a screen is looked at by someone who cannot look, and the record in
//! the roadmap of what a screen looks like comes from it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::window::{CursorMoved, PresentMode, PrimaryWindow, WindowEvent};

use crate::screens::AppScreen;

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        let dev = Dev::from_env();
        if dev.screen.is_some() || dev.screenshot.is_some() {
            info!("dev: screen {:?}, screenshot {:?}", dev.screen, dev.screenshot);
        }
        let wants_screenshot = dev.screenshot.is_some();
        app.insert_resource(dev);
        if wants_screenshot {
            app.add_systems(Startup, present_without_vsync).add_systems(Update, screenshot_then_exit);
        }
    }
}

/// The window `NETRUNNER_WINDOW` asked for, if any.
///
/// Read here rather than in `main`, because this module owns every
/// environment variable the client reads — and read before the `App` is
/// built, which is why it is a function and not a field of [`Dev`]: the
/// window is decided when `WindowPlugin` is configured, before any
/// resource exists.
pub fn window_size() -> Option<(u32, u32)> {
    let spec = std::env::var("NETRUNNER_WINDOW").ok()?;
    let spec = spec.trim().to_ascii_lowercase();
    let (width, height) = spec.split_once('x')?;
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

/// Which card the menu hook opens its menu over.
///
/// `1` is the hand card it has always taken, and the other two exist
/// because that one could not show the bug it was asked to: a hand card
/// sits on the window's bottom edge with a handful of entries, which is
/// the one case a menu never ran off the screen in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPick {
    /// The first hand card with an action, else the first card.
    Hand,
    /// The target with the most entries: the tallest menu on the board.
    Most,
    /// The target nearest the window's top edge that has an entry: the
    /// menu that has to flip below its card.
    Top,
}

impl MenuPick {
    fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "" => None,
            "most" => Some(MenuPick::Most),
            "top" => Some(MenuPick::Top),
            _ => Some(MenuPick::Hand),
        }
    }
}

/// What the environment asked for.
#[derive(Resource, Debug, Default)]
pub struct Dev {
    pub screen: Option<AppScreen>,
    /// The chair a dev game seats the person in.
    pub game: Option<netrunner_core::rules::Side>,
    /// A lesson to boot into, by id, and whether its words are put away.
    pub lesson: Option<String>,
    pub begin: bool,
    /// The dev game's decks, by id, when not the defaults.
    pub corp_deck: Option<String>,
    pub runner_deck: Option<String>,
    /// How many decisions the board takes by itself, and how many it has.
    pub autoplay: u32,
    pub autoplayed: u32,
    /// Open the options window on the board, once.
    pub options: bool,
    /// Open the list of keys on the board, once.
    pub keys: bool,
    /// Open the timing chart once, as `keys` opens the list of keys.
    pub timing: bool,
    /// Pick up the first hand card that has somewhere to go, once.
    pub drag: bool,
    /// Open the actions menu over a card, once.
    pub menu: Option<MenuPick>,
    /// Lift the first hand card out of the hand as a hover would.
    pub lift: bool,
    /// Open the `n`th readable card in the reader, once (1-based).
    pub read: Option<usize>,
    /// Open the deck editor's identity picker, once.
    pub identities: bool,
    /// Hold a run at its first encounter for the screenshot.
    pub hold_run: bool,
    /// Stop the autoplay at the first encounter the person is asked
    /// anything in, whether or not their rig can break it.
    pub hold_ice: bool,
    /// Stop the autoplay at the first card-selection prompt.
    pub hold_selection: bool,
    /// Stop the autoplay at the first "where to install" a card asks.
    pub hold_install: bool,
    /// Stop the autoplay at the first card the person is asked to access.
    pub hold_access: bool,
    /// Stop the autoplay at the first encounter a card of the person's
    /// can break the whole of.
    pub hold_break: bool,
    /// Host a Trojan whenever one can be, and stop once one is hosted.
    pub hold_trojan: bool,
    /// Stop the autoplay at the first card's "you may" the pop-up offers
    /// to answer for good (`netrunner_client::standing`).
    pub hold_may: bool,
    /// `NETRUNNER_HOLD_RUN_PASS`: `Some(false)` holds at the offer,
    /// `Some(true)` presses it there.
    pub hold_run_pass: Option<bool>,
    /// Open the sheet of the first installed Corp card, once.
    pub sheet: bool,
    /// Open this zone's sheet once, and with `true` its first card.
    pub pile: Option<(netrunner_client::board::Target, bool)>,
    /// Open this side's score area with its first row expanded, once.
    pub agendas: Option<netrunner_core::rules::Side>,
    pub screenshot: Option<PathBuf>,
    /// A record to open on the replay board, and where in it.
    pub replay: Option<(PathBuf, Option<netrunner_client::replay::Start>)>,
    /// `(x, y, lines)`.
    pub scroll: Option<(f32, f32, f32)>,
    /// Open this drop-down (1-based, in reading order) before the shot.
    pub dropdown: Option<usize>,
    /// Draw the board's rows over the table, so a field can be painted
    /// to where the cards actually fall rather than guessed at.
    pub table_guide: bool,
    frames: u32,
    /// Set by the screenshot's observer once the file is written.
    saved: Arc<AtomicBool>,
    exiting: bool,
}

impl Dev {
    fn from_env() -> Self {
        let scroll = std::env::var("NETRUNNER_SCROLL").ok().and_then(|spec| {
            let parts: Vec<f32> = spec.split(',').filter_map(|part| part.trim().parse().ok()).collect();
            (parts.len() == 3).then(|| (parts[0], parts[1], parts[2]))
        });
        let game = std::env::var("NETRUNNER_GAME").ok().and_then(|side| match side.trim().to_ascii_lowercase().as_str() {
            "corp" => Some(netrunner_core::rules::Side::Corp),
            "runner" => Some(netrunner_core::rules::Side::Runner),
            _ => None,
        });
        Dev {
            screen: std::env::var("NETRUNNER_SCREEN").ok().and_then(|name| AppScreen::from_name(&name)),
            game,
            lesson: std::env::var("NETRUNNER_LESSON").ok().filter(|id| !id.trim().is_empty()),
            begin: std::env::var_os("NETRUNNER_BEGIN").is_some_and(|v| !v.is_empty()),
            corp_deck: std::env::var("NETRUNNER_CORP_DECK").ok().filter(|id| !id.trim().is_empty()),
            runner_deck: std::env::var("NETRUNNER_RUNNER_DECK").ok().filter(|id| !id.trim().is_empty()),
            autoplay: std::env::var("NETRUNNER_AUTOPLAY").ok().and_then(|n| n.trim().parse().ok()).unwrap_or(0),
            autoplayed: 0,
            dropdown: std::env::var("NETRUNNER_DROPDOWN").ok().and_then(|n| n.trim().parse().ok()).filter(|n| *n > 0),
            options: std::env::var_os("NETRUNNER_OPTIONS").is_some_and(|v| !v.is_empty()),
            keys: std::env::var_os("NETRUNNER_KEYS").is_some_and(|v| !v.is_empty()),
            timing: std::env::var_os("NETRUNNER_TIMING").is_some_and(|v| !v.is_empty()),
            drag: std::env::var_os("NETRUNNER_DRAG").is_some_and(|v| !v.is_empty()),
            menu: std::env::var("NETRUNNER_MENU").ok().and_then(|pick| MenuPick::from_name(&pick)),
            lift: std::env::var_os("NETRUNNER_LIFT").is_some_and(|v| !v.is_empty()),
            read: std::env::var("NETRUNNER_READ").ok().and_then(|n| n.trim().parse().ok()).filter(|n| *n > 0),
            identities: std::env::var_os("NETRUNNER_IDENTITIES").is_some_and(|v| !v.is_empty()),
            hold_run: std::env::var_os("NETRUNNER_HOLD_RUN").is_some_and(|v| !v.is_empty()),
            hold_ice: std::env::var_os("NETRUNNER_HOLD_ICE").is_some_and(|v| !v.is_empty()),
            hold_selection: std::env::var_os("NETRUNNER_HOLD_SELECTION").is_some_and(|v| !v.is_empty()),
            hold_install: std::env::var_os("NETRUNNER_HOLD_INSTALL").is_some_and(|v| !v.is_empty()),
            hold_access: std::env::var_os("NETRUNNER_HOLD_ACCESS").is_some_and(|v| !v.is_empty()),
            hold_break: std::env::var_os("NETRUNNER_HOLD_BREAK").is_some_and(|v| !v.is_empty()),
            hold_trojan: std::env::var_os("NETRUNNER_HOLD_TROJAN").is_some_and(|v| !v.is_empty()),
            hold_may: std::env::var_os("NETRUNNER_HOLD_MAY").is_some_and(|v| !v.is_empty()),
            hold_run_pass: std::env::var("NETRUNNER_HOLD_RUN_PASS").ok().filter(|v| !v.is_empty()).map(|v| v.trim() == "on"),
            sheet: std::env::var_os("NETRUNNER_SHEET").is_some_and(|v| !v.is_empty()),
            pile: std::env::var("NETRUNNER_PILE").ok().and_then(|spec| {
                use netrunner_client::board::{Pile, Target};
                use netrunner_core::rules::ServerId;
                let spec = spec.trim().to_ascii_lowercase();
                let (zone, card) = match spec.strip_suffix("+card") {
                    Some(zone) => (zone.to_string(), true),
                    None => (spec, false),
                };
                let target = match zone.as_str() {
                    "archives" => Target::Server(ServerId::Archives),
                    "heap" => Target::Pile(Pile::Heap),
                    "hq" => Target::Server(ServerId::Hq),
                    _ => return None,
                };
                Some((target, card))
            }),
            agendas: std::env::var("NETRUNNER_AGENDAS").ok().and_then(|side| match side.trim().to_ascii_lowercase().as_str() {
                "corp" => Some(netrunner_core::rules::Side::Corp),
                "runner" => Some(netrunner_core::rules::Side::Runner),
                _ => None,
            }),
            screenshot: std::env::var_os("NETRUNNER_SCREENSHOT").map(PathBuf::from),
            replay: std::env::var_os("NETRUNNER_REPLAY").filter(|path| !path.is_empty()).map(|path| {
                (PathBuf::from(path), std::env::var("NETRUNNER_REPLAY_AT").ok().and_then(|at| at.parse().ok()))
            }),
            scroll,
            table_guide: std::env::var_os("NETRUNNER_TABLE_GUIDE").is_some_and(|v| !v.is_empty()),
            frames: 0,
            saved: Arc::new(AtomicBool::new(false)),
            exiting: false,
        }
    }

    /// Where boot goes: the requested screen, else the board when a dev
    /// game was asked for, else the menu.
    pub fn first_screen(&self) -> AppScreen {
        // A replay is looked at on the board, which the replays screen
        // hands it to as soon as it has opened the record.
        match self.named_screen() {
            Some(AppScreen::Replay) if self.screen.is_none() => AppScreen::Game,
            named => named.unwrap_or(AppScreen::MainMenu),
        }
    }

    /// The screen a hook asked for, if any — `NETRUNNER_SCREEN`, the
    /// board for `NETRUNNER_GAME`, or the replays (which open the record
    /// on the board) for `NETRUNNER_REPLAY` — which boot goes to without
    /// a splash.
    pub fn named_screen(&self) -> Option<AppScreen> {
        self.screen.or((self.game.is_some() || self.lesson.is_some()).then_some(AppScreen::Game)).or(self.replay.is_some().then_some(AppScreen::Replay))
    }
}

/// A window nobody is looking at gets no frames. Under Wayland the
/// compositor withholds frame callbacks from a surface it is not
/// showing, and with vsync on, Vulkan's present blocks on them: the
/// render thread sat in `queue_present` and the third frame never came,
/// which took a stack trace to find. Presenting without vsync does not
/// wait. (`MESA_VK_WSI_PRESENT_MODE=immediate` is the same thing from
/// outside, if a driver ignores the request.)
fn present_without_vsync(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut window in &mut windows {
        window.present_mode = PresentMode::AutoNoVsync;
    }
}

/// Text lays out over the first frames and the font arrives a little
/// later; thirty frames is past both. The exit waits for another thirty
/// frames *and* the save: thirty frames alone was not always longer than
/// the save took, and with the card browser's scans decoding on every
/// core the exit came first and no file was written.
const SCREENSHOT_FRAME: u32 = 30;
const EXIT_FRAME: u32 = 60;
/// The pointer is placed, then the wheel turned two frames later, so
/// picking has a location before the scroll arrives.
const POINTER_FRAME: u32 = 15;
/// The drop-down is opened once the screen is laid out, since its list
/// is measured against where its head was drawn.
const DROPDOWN_FRAME: u32 = 10;
const WHEEL_FRAME: u32 = 17;

fn screenshot_then_exit(
    mut commands: Commands,
    mut dev: ResMut<Dev>,
    screen: Res<State<AppScreen>>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut window_events: MessageWriter<WindowEvent>,
    scroll_areas: Query<(Entity, &ComputedNode, &ScrollPosition), With<bevy::ui_widgets::ScrollArea>>,
    heads: Query<(Entity, &UiGlobalTransform), With<crate::widgets::dropdown::Head>>,
    mut pressed: MessageWriter<crate::widgets::Pressed>,
    mut exit: MessageWriter<AppExit>,
) {
    // A card held for the shot (`NETRUNNER_DRAG`) waits for the person's
    // own action phase, which may be several opponent turns away, so the
    // frames do not start counting until it is in hand.
    if *screen.get() != dev.first_screen() || dev.autoplayed < dev.autoplay || dev.drag {
        // The frames are counted from when the screen has nothing left
        // to do by itself, so an autoplayed board is shot after its last
        // decision, not during it.
        return;
    }
    dev.frames += 1;
    if dev.frames == DROPDOWN_FRAME
        && let Some(n) = dev.dropdown
    {
        let mut order: Vec<(Entity, Vec2)> = heads.iter().map(|(entity, at)| (entity, at.translation)).collect();
        order.sort_by(|a, b| a.1.y.total_cmp(&b.1.y).then(a.1.x.total_cmp(&b.1.x)));
        match order.get(n - 1) {
            Some((head, _)) => {
                info!("dev: opening drop-down {n} of {}", order.len());
                // The widget's own message, not an `Interaction`: the
                // focus system would set that back before it was read.
                pressed.write(crate::widgets::Pressed(*head));
            }
            None => warn!("dev: NETRUNNER_DROPDOWN={n}, but the screen has {} drop-downs", order.len()),
        }
    }
    if let (Some((x, y, lines)), Ok(window)) = (dev.scroll, windows.single()) {
        if dev.frames == POINTER_FRAME {
            window_events.write(WindowEvent::CursorMoved(CursorMoved { window, position: Vec2::new(x, y), delta: None }));
        }
        if dev.frames == WHEEL_FRAME {
            info!("dev: wheel {lines} lines at ({x}, {y})");
            window_events.write(WindowEvent::MouseWheel(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: -lines, window, phase: TouchPhase::Moved }));
        }
    }
    if dev.frames == SCREENSHOT_FRAME
        && let Some(path) = dev.screenshot.clone()
    {
        for (entity, node, position) in &scroll_areas {
            let size = node.size() * node.inverse_scale_factor;
            let content = node.content_size() * node.inverse_scale_factor;
            info!("dev: scroll area {entity}: size {size:?}, content {content:?}, position {:?}", position.0);
        }
        info!("dev: taking the screenshot");
        let mut save = save_to_disk(path);
        let saved = dev.saved.clone();
        commands.spawn(Screenshot::primary_window()).observe(move |captured: On<ScreenshotCaptured>| {
            save(captured);
            saved.store(true, Ordering::Release);
        });
    }
    if dev.frames >= EXIT_FRAME && (dev.screenshot.is_none() || dev.saved.load(Ordering::Acquire)) && !dev.exiting {
        dev.exiting = true;
        info!("dev: exiting");
        exit.write(AppExit::Success);
    }
}
