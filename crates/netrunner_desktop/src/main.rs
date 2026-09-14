//! The graphical desktop client. Everything is in the library so the
//! plugin set a test builds is the one this binary runs.

use bevy::prelude::*;
use bevy::render::settings::{PowerPreference, RenderCreation, WgpuSettings};
use bevy::render::RenderPlugin;
use bevy::window::WindowResolution;

use netrunner_desktop::{NetrunnerDesktopPlugins, WINDOW_TITLE};

fn main() -> AppExit {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: WINDOW_TITLE.to_string(),
                        resolution: WindowResolution::new(1280, 800),
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(wgpu_settings())), ..default() }),
        )
        .add_plugins(NetrunnerDesktopPlugins)
        .run()
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
