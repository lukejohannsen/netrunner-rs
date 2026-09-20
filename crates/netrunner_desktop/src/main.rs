//! The graphical desktop client. Everything is in the library so the
//! plugin set a test builds is the one this binary runs.

use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::settings::{PowerPreference, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::{MonitorSelection, WindowMode, WindowResolution};

use netrunner_desktop::{NetrunnerDesktopPlugins, WINDOW_TITLE};

fn main() -> AppExit {
    // A dev window, when one was asked for: the board is fullscreen by
    // design, so a screenshot of one cramped for room — a menu or a
    // pop-up with more in it than the window holds — cannot be taken any
    // other way. Nothing but `NETRUNNER_WINDOW` reaches this.
    let dev_window = netrunner_desktop::dev::window_size();
    let (mode, resolution) = match dev_window {
        Some((width, height)) => (WindowMode::Windowed, WindowResolution::new(width, height)),
        None => (WindowMode::BorderlessFullscreen(MonitorSelection::Current), WindowResolution::new(1280, 800)),
    };
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: WINDOW_TITLE.to_string(),
                        // The game is played fullscreen and nothing else:
                        // the board fits whatever the monitor is
                        // (`models::layout`), and a card table in a window
                        // among other windows is not the thing asked for.
                        // Borderless rather than exclusive, so alt-tab and
                        // the compositor behave; the resolution is what a
                        // window would be if a platform refuses.
                        mode,
                        resolution,
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(wgpu_settings())), ..default() })
                .set(LogPlugin { filter: log_filter(), ..default() }),
        )
        .add_plugins(NetrunnerDesktopPlugins)
        .run()
}

/// Bevy's default filter, with rodio's output-stream errors turned off.
///
/// The audio plugin opens the output stream at start-up and holds it
/// idle until something plays, and on this box (PipeWire behind ALSA)
/// that idle stream reports `alsa::poll() returned POLLERR` at random,
/// as an ERROR line, again and again — a stream nothing has written to
/// cannot have failed in any way a player would notice, and the log was
/// unreadable behind it. The sound bank (§4) revisits this if a stream
/// that *is* playing shows the same; `RUST_LOG` still overrides the
/// whole filter for anyone chasing an audio fault.
fn log_filter() -> String {
    format!("{},rodio::stream=off", bevy::log::DEFAULT_FILTER)
}

/// Bevy's defaults, except that the adapter is the *low-power* one unless
/// `WGPU_POWER_PREF` says otherwise.
///
/// Bevy asks for the high-performance adapter, which on a laptop with two
/// GPUs is the discrete one — and on the box this was built on (Wayland,
/// NVIDIA 550 beside an Intel iGPU) that adapter enumerates, creates the
/// window, and then fails every frame with "Surface::configure: Invalid
/// surface", because the discrete GPU cannot present to a surface the
/// compositor owns on the integrated one. A card game draws two rows of
/// rectangles and text; the integrated GPU is more than enough for it and
/// is the one the display is wired to. The environment variable is still
/// honoured, so `WGPU_POWER_PREF=high` gets the old behaviour back.
fn wgpu_settings() -> WgpuSettings {
    let mut settings = WgpuSettings::default();
    if std::env::var_os("WGPU_POWER_PREF").is_none() {
        settings.power_preference = PowerPreference::LowPower;
    }
    settings
}
