//! Driving the client without a hand on it, for whoever cannot see the
//! window: a model checking its own work, or a CI job one day.
//!
//! Two environment variables, both ignored when unset:
//!
//! - `NETRUNNER_SCREEN=<AppScreen>` — boot goes to that screen instead of
//!   the menu (`cards`, `settings`, … — the variant name, any case).
//! - `NETRUNNER_SCREENSHOT=<path.png>` — once the screen has had time to
//!   lay out and draw, the window is saved there and the client exits.
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

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
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

/// What the environment asked for.
#[derive(Resource, Debug, Default)]
pub struct Dev {
    pub screen: Option<AppScreen>,
    pub screenshot: Option<PathBuf>,
    /// `(x, y, lines)`.
    pub scroll: Option<(f32, f32, f32)>,
    frames: u32,
}

impl Dev {
    fn from_env() -> Self {
        let scroll = std::env::var("NETRUNNER_SCROLL").ok().and_then(|spec| {
            let parts: Vec<f32> = spec.split(',').filter_map(|part| part.trim().parse().ok()).collect();
            (parts.len() == 3).then(|| (parts[0], parts[1], parts[2]))
        });
        Dev {
            screen: std::env::var("NETRUNNER_SCREEN").ok().and_then(|name| AppScreen::from_name(&name)),
            screenshot: std::env::var_os("NETRUNNER_SCREENSHOT").map(PathBuf::from),
            scroll,
            frames: 0,
        }
    }

    /// Where boot goes: the requested screen, else the menu.
    pub fn first_screen(&self) -> AppScreen {
        self.screen.unwrap_or(AppScreen::MainMenu)
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
/// later; thirty frames is past both. The exit follows after another
/// thirty, which is longer than the save takes.
const SCREENSHOT_FRAME: u32 = 30;
const EXIT_FRAME: u32 = 60;
/// The pointer is placed, then the wheel turned two frames later, so
/// picking has a location before the scroll arrives.
const POINTER_FRAME: u32 = 15;
const WHEEL_FRAME: u32 = 17;

fn screenshot_then_exit(
    mut commands: Commands,
    mut dev: ResMut<Dev>,
    screen: Res<State<AppScreen>>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut window_events: MessageWriter<WindowEvent>,
    scroll_areas: Query<(Entity, &ComputedNode, &ScrollPosition), With<bevy::ui_widgets::ScrollArea>>,
    mut exit: MessageWriter<AppExit>,
) {
    if *screen.get() != dev.first_screen() {
        return;
    }
    dev.frames += 1;
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
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if dev.frames == EXIT_FRAME {
        info!("dev: exiting");
        exit.write(AppExit::Success);
    }
}
