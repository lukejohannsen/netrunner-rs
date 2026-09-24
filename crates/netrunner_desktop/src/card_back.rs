//! The drawn card backs: the procedural tier, so the client has a back
//! for each side before any file exists.
//!
//! The Corp's is blue and the Runner's red, as Null Signal Games prints
//! them. The pattern is a rim, an inner line, a grid of circuit traces
//! with pads at the crossings, and a diamond in the middle — enough to
//! read as a card back at thumbnail size and not to pretend to be the
//! official art. The shipped printings under `assets/cards/backs/`, and a
//! player's drop-in, replace it
//! (`assets/cards/README.md`).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use netrunner_core::rules::Side;

use crate::theme::Theme;

pub const WIDTH: u32 = 250;
pub const HEIGHT: u32 = 350;

/// The back for `side`, as an RGBA image at 5:7.
pub fn paint(side: Side, theme: &Theme) -> Image {
    let base = theme.side(side).to_srgba();
    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let tint = shade(x, y);
            data.extend_from_slice(&[channel(base.red, tint), channel(base.green, tint), channel(base.blue, tint), 255]);
        }
    }
    Image::new(
        Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// How bright a pixel is relative to the side's colour: the rim dark, the
/// inner line bright, the ground darker still, the traces between.
fn shade(x: u32, y: u32) -> f32 {
    const RIM: u32 = 8;
    const LINE: u32 = 13;
    let edge = x.min(y).min(WIDTH - 1 - x).min(HEIGHT - 1 - y);
    if edge < RIM {
        return 0.55;
    }
    if edge < LINE {
        return 1.15;
    }
    let (cx, cy) = (WIDTH as i64 / 2, HEIGHT as i64 / 2);
    let d = (x as i64 - cx).abs() + (y as i64 - cy).abs();
    if d < 30 {
        return 0.5;
    }
    if d < 42 {
        return 1.0;
    }
    let on_trace = |v: u32| (v + 12).is_multiple_of(24);
    let near_pad = |v: u32| ((v + 12) % 24) <= 1 || ((v + 12) % 24) >= 23;
    if near_pad(x) && near_pad(y) {
        return 1.05;
    }
    if on_trace(x) || on_trace(y) {
        return 0.6;
    }
    0.35
}

fn channel(value: f32, tint: f32) -> u8 {
    ((value * tint).clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_back_is_the_right_size_and_the_sides_differ() {
        let theme = Theme::default();
        let corp = paint(Side::Corp, &theme);
        let runner = paint(Side::Runner, &theme);
        assert_eq!(corp.texture_descriptor.size.width, WIDTH);
        assert_eq!(corp.texture_descriptor.size.height, HEIGHT);
        assert_eq!(corp.data.as_ref().map(Vec::len), Some((WIDTH * HEIGHT * 4) as usize));
        assert_ne!(corp.data, runner.data);
        // The centre of the diamond is darker than the rim's inner line.
        let px = |image: &Image, x: u32, y: u32| {
            let i = ((y * WIDTH + x) * 4) as usize;
            let data = image.data.as_ref().unwrap();
            (data[i] as u32) + (data[i + 1] as u32) + (data[i + 2] as u32)
        };
        assert!(px(&corp, WIDTH / 2, HEIGHT / 2) < px(&corp, 10, 10));
    }
}
