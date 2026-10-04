//! Renders the client's sound effects: `cargo run -p netrunner_desktop
//! --example render_sfx -- <dir>` writes one 44.1 kHz mono WAV per sound
//! into `<dir>`, and `scripts/render_sfx.sh` turns those into the `.ogg`
//! files committed under `assets/sfx/`.
//!
//! **The sounds are this project's, made here** (decided 4 October 2026):
//! the person asked for a cyberpunk theme in place of the recorded paper
//! cards and poker chips, and a set made in code is one palette — low
//! saws behind a swept filter, a sub for weight, filtered and crushed
//! noise for air and glitch, struck metal for a part moving — that can
//! be retuned one
//! recipe at a time when a sound is wrong. Every seed is fixed, so a
//! render is the same samples each time and a new file in `assets/sfx/` is a
//! change someone made here.
//!
//! This is not the client's synthesized tier (`audio::synth`), which is
//! deliberately plain and stands in only for a file nobody installed.
//! Nothing here is compiled into the client.

use std::f32::consts::{PI, TAU};
use std::path::Path;

const SR: f32 = 44_100.0;

/// Silence written after every sound, for the encoder to lose.
const TRAILING_SILENCE: f32 = 0.2;

type Buf = Vec<f32>;

fn n(seconds: f32) -> usize {
    (seconds * SR) as usize
}

/// xorshift32: the one source of noise, seeded per sound.
struct Rng(u32);

impl Rng {
    /// 0..1
    fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32
    }

    /// -1..1
    fn bipolar(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Saw,
}

/// The band-limiting step under a saw's edge, without
/// which a glide to 6 kHz aliases into a whine.
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// An oscillator whose pitch is a function of time, started at `phase`
/// (0..1).
fn tone(wave: Wave, seconds: f32, mut phase: f32, freq: &impl Fn(f32) -> f32) -> Buf {
    (0..n(seconds))
        .map(|i| {
            let dt = freq(i as f32 / SR) / SR;
            let out = match wave {
                Wave::Sine => (TAU * phase).sin(),
                Wave::Saw => 2.0 * phase - 1.0 - blep(phase, dt),
            };
            phase = (phase + dt).fract();
            out
        })
        .collect()
}

/// Five saws a few cents apart, each from its own phase: the thick lead
/// every synthwave track is built on.
fn supersaw(seconds: f32, cents: f32, freq: &impl Fn(f32) -> f32) -> Buf {
    let mut out = vec![0.0; n(seconds)];
    for voice in 0..5 {
        let spread = (voice as f32 - 2.0) / 2.0;
        let ratio = 2f32.powf(spread * cents / 1200.0);
        let layer = tone(Wave::Saw, seconds, (voice as f32 * 0.37).fract(), &|t| freq(t) * ratio);
        for (slot, sample) in out.iter_mut().zip(layer) {
            *slot += sample / 5.0;
        }
    }
    out
}

/// A pitch that slides from one frequency to another, evenly by ear.
fn glide(from: f32, to: f32, seconds: f32) -> impl Fn(f32) -> f32 {
    move |t| from * (to / from).powf((t / seconds).clamp(0.0, 1.0))
}

/// Struck metal: a sine bent by a modulator at a ratio no harmonic
/// series holds, so it rings like a latch or a relay and never like a
/// note. The modulation dies faster than the tone, so the strike is
/// rough and the tail is plain.
fn metal(seconds: f32, freq: f32, index: f32, ring: f32) -> Buf {
    (0..n(seconds))
        .map(|i| {
            let t = i as f32 / SR;
            let bend = index * (-t / (ring * 0.4)).exp();
            let modulation = bend * ((TAU * freq * 2.76 * t).sin() + 0.6 * (TAU * freq * 5.4 * t).sin());
            (TAU * freq * t + modulation).sin() * (-t / ring).exp()
        })
        .collect()
}

/// A tick with a pitch: one burst of noise through a narrow filter,
/// which rings at `freq` for as long as `q` lets it. The sound of a
/// small part moving, where a tone would be the sound of a tune.
fn ping(seconds: f32, freq: f32, q: f32, rng: &mut Rng) -> Buf {
    let mut burst = vec![0.0; n(seconds)];
    for sample in burst.iter_mut().take(n(0.0015)) {
        *sample = rng.bipolar();
    }
    filter(&burst, Pass::Band, q, |_| freq)
}

fn noise(seconds: f32, rng: &mut Rng) -> Buf {
    (0..n(seconds)).map(|_| rng.bipolar()).collect()
}

/// A modem's chatter: a square flipping between two pitches every few
/// milliseconds.
fn chatter(seconds: f32, rng: &mut Rng, low: f32, high: f32) -> Buf {
    let total = n(seconds);
    let mut out = Vec::with_capacity(total);
    let mut phase = 0.0f32;
    while out.len() < total {
        let freq = if rng.unit() < 0.5 { low } else { high };
        for _ in 0..n(0.008 + 0.017 * rng.unit()) {
            out.push(if phase < 0.5 { 1.0 } else { -1.0 });
            phase = (phase + freq / SR).fract();
        }
    }
    out.truncate(total);
    out
}

/// Sparse clicks: a connection dropping.
fn crackle(seconds: f32, rng: &mut Rng, per_second: f32) -> Buf {
    (0..n(seconds)).map(|_| if rng.unit() < per_second / SR { rng.bipolar() } else { 0.0 }).collect()
}

/// Multiplies `buf` by a loudness that is a function of time and of the
/// buffer's whole length.
fn shape(mut buf: Buf, envelope: impl Fn(f32, f32) -> f32) -> Buf {
    let length = buf.len() as f32 / SR;
    for (i, sample) in buf.iter_mut().enumerate() {
        *sample *= envelope(i as f32 / SR, length);
    }
    buf
}

/// A struck sound: up in `attack`, then dying with time constant `ring`.
fn struck(buf: Buf, attack: f32, ring: f32) -> Buf {
    shape(buf, |t, _| (t / attack).min(1.0) * (-(t - attack).max(0.0) / ring).exp())
}

/// A sound that swells and falls.
fn hump(buf: Buf) -> Buf {
    shape(buf, |t, length| (PI * t / length).sin())
}

#[derive(Clone, Copy)]
enum Pass {
    Low,
    Band,
}

/// A state-variable filter (the trapezoidal form, which stays stable
/// while its cutoff is swept) with the cutoff a function of time.
fn filter(buf: &[f32], pass: Pass, q: f32, cutoff: impl Fn(f32) -> f32) -> Buf {
    let (mut ic1, mut ic2) = (0.0f32, 0.0f32);
    let k = 1.0 / q;
    buf.iter()
        .enumerate()
        .map(|(i, &x)| {
            let g = (PI * cutoff(i as f32 / SR).clamp(20.0, 18_000.0) / SR).tan();
            let a1 = 1.0 / (1.0 + g * (g + k));
            let a2 = g * a1;
            let a3 = g * a2;
            let v3 = x - ic2;
            let v1 = a1 * ic1 + a2 * v3;
            let v2 = ic2 + a2 * ic1 + a3 * v3;
            ic1 = 2.0 * v1 - ic1;
            ic2 = 2.0 * v2 - ic2;
            match pass {
                Pass::Low => v2,
                Pass::Band => v1,
            }
        })
        .collect()
}

/// Sample-and-hold over `hold(t)` samples, then `levels` steps of
/// loudness: the sound of a cheap converter.
fn crush(buf: &[f32], levels: f32, hold: impl Fn(f32) -> f32) -> Buf {
    let mut held = 0.0;
    let mut left = 0.0f32;
    buf.iter()
        .enumerate()
        .map(|(i, &x)| {
            if left <= 0.0 {
                held = (x * levels).round() / levels;
                left += hold(i as f32 / SR).max(1.0);
            }
            left -= 1.0;
            held
        })
        .collect()
}

fn drive(buf: Buf, amount: f32) -> Buf {
    buf.into_iter().map(|x| (x * amount).tanh()).collect()
}

/// Layers, each starting at its own time with its own gain.
fn mix(layers: Vec<(f32, f32, Buf)>) -> Buf {
    let length = layers.iter().map(|(start, _, buf)| n(*start) + buf.len()).max().unwrap_or(0);
    let mut out = vec![0.0; length];
    for (start, gain, buf) in layers {
        for (slot, sample) in out[n(start)..].iter_mut().zip(buf) {
            *slot += sample * gain;
        }
    }
    out
}

fn echo(buf: &[f32], delay: f32, feedback: f32, tail: f32) -> Buf {
    let step = n(delay);
    let mut out = buf.to_vec();
    out.resize(buf.len() + n(tail), 0.0);
    for i in step..out.len() {
        out[i] += out[i - step] * feedback;
    }
    out
}

/// A small room (four damped combs into two all-passes), mixed under
/// the dry sound: what stops a synthesized sound from sitting on the
/// speaker cone.
fn room(buf: &[f32], wet: f32, decay: f32, tail: f32) -> Buf {
    let length = buf.len() + n(tail);
    let mut dry = buf.to_vec();
    dry.resize(length, 0.0);
    let mut sum = vec![0.0f32; length];
    for delay in [0.0297, 0.0371, 0.0411, 0.0437] {
        let step = n(delay);
        let mut line = vec![0.0f32; length];
        let mut damp = 0.0f32;
        for i in 0..length {
            let back = if i >= step { line[i - step] } else { 0.0 };
            damp += 0.45 * (back - damp);
            line[i] = dry[i] + damp * decay;
            sum[i] += back / 4.0;
        }
    }
    for delay in [0.005, 0.0017] {
        let step = n(delay);
        let input = sum.clone();
        for i in 0..length {
            let delayed_in = if i >= step { input[i - step] } else { 0.0 };
            let delayed_out = if i >= step { sum[i - step] } else { 0.0 };
            sum[i] = -0.7 * input[i] + delayed_in + 0.7 * delayed_out;
        }
    }
    dry.iter().zip(sum).map(|(d, w)| d + w * wet).collect()
}

/// Edges that do not click, and the peak the sound is heard at.
fn finish(mut buf: Buf, peak: f32) -> Buf {
    let last = buf.len().saturating_sub(1);
    let (rise, fall) = (n(0.0005).max(1), n(0.008).min(buf.len() / 3).max(1));
    for (i, sample) in buf.iter_mut().enumerate() {
        *sample *= (i as f32 / rise as f32).min(1.0) * ((last - i) as f32 / fall as f32).min(1.0);
    }
    let loudest = buf.iter().fold(0.0f32, |max, x| max.max(x.abs())).max(1e-6);
    buf.iter_mut().for_each(|x| *x *= peak / loudest);
    buf
}

// The loudness each family is heard at: the interface under the cards,
// the cards under the moments of a run.
const QUIET: f32 = 0.42;
const TABLE: f32 = 0.6;
const EVENT: f32 = 0.82;

/// Equal temperament from A4: `note(0)` is 440 Hz.
fn note(semitones: i32) -> f32 {
    440.0 * 2f32.powf(semitones as f32 / 12.0)
}

// **The palette is dark** (the person's verdict on the first render,
// 4 October 2026: "too bright, like a happy Mario game and not
// futuristic tech"). So: nothing here plays a tune. No bells, no bare
// square-wave blips, no climbing notes and no major third anywhere —
// those are a console's vocabulary. A sound is weight (a sub), air
// (filtered noise), a part moving (a ping, struck metal) or a drone
// behind a closing filter, and where two pitches sound together they
// are a fifth, an octave or a tritone.

/// A sub hit: a sine that drops as it lands.
fn thump(seconds: f32, from: f32, to: f32, ring: f32) -> Buf {
    struck(tone(Wave::Sine, seconds, 0.0, &glide(from, to, seconds * 0.4)), 0.002, ring)
}

/// Air moving through a filter that opens or closes.
fn whoosh(seconds: f32, from: f32, to: f32, q: f32, rng: &mut Rng) -> Buf {
    hump(filter(&noise(seconds, rng), Pass::Band, q, glide(from, to, seconds)))
}

/// Low saws on `steps`, behind a filter that moves from `dark` to
/// `open`: the theme's one sustained voice.
fn drone(steps: &[i32], seconds: f32, bend: f32, cutoff: impl Fn(f32) -> f32) -> Buf {
    let layers = steps
        .iter()
        .map(|step| {
            let pitch = note(*step);
            (0.0, 1.0, supersaw(seconds, 20.0, &|t: f32| pitch * 2f32.powf(bend * (t / seconds).powi(2) / 12.0)))
        })
        .collect();
    filter(&mix(layers), Pass::Low, 1.6, cutoff)
}

/// A press: a relay's tick.
fn click() -> Buf {
    let mut rng = Rng(7);
    let tick = ping(0.03, 950.0, 5.0, &mut rng);
    let body = thump(0.03, 160.0, 90.0, 0.006);
    finish(mix(vec![(0.0, 1.0, tick), (0.0, 0.5, body)]), QUIET)
}

/// A switch thrown: the tick, and its contact a moment later.
fn toggle() -> Buf {
    let mut rng = Rng(13);
    let first = ping(0.04, 620.0, 6.0, &mut rng);
    let second = ping(0.04, 830.0, 6.0, &mut rng);
    finish(mix(vec![(0.0, 1.0, first), (0.03, 0.7, second), (0.0, 0.4, thump(0.03, 140.0, 80.0, 0.006))]), QUIET)
}

/// One screen to another: air through an opening filter over a low
/// swell, up for forward and down for back, so the two are one gesture
/// in two directions.
fn sweep(up: bool) -> Buf {
    let seconds = 0.16;
    let mut rng = Rng(11);
    let (dark, bright) = (260.0, 1900.0);
    let air = if up { whoosh(seconds, dark, bright, 2.5, &mut rng) } else { whoosh(seconds, bright, dark, 2.5, &mut rng) };
    let (from, to) = if up { (55.0, 98.0) } else { (98.0, 55.0) };
    let swell = hump(tone(Wave::Sine, seconds, 0.0, &glide(from, to, seconds)));
    finish(room(&mix(vec![(0.0, 1.0, air), (0.0, 0.9, swell)]), 0.2, 0.55, 0.1), QUIET)
}

/// A panel sliding open or shut: a servo's air, and the latch at the
/// end of an opening or the start of a closing.
fn panel(open: bool) -> Buf {
    let seconds = 0.15;
    let mut rng = Rng(23);
    let (low, high) = (380.0, 1500.0);
    let air = if open { whoosh(seconds, low, high, 5.0, &mut rng) } else { whoosh(seconds, high, low, 5.0, &mut rng) };
    let latch = mix(vec![(0.0, 1.0, metal(0.06, 210.0, 2.2, 0.014)), (0.0, 0.7, thump(0.05, 120.0, 60.0, 0.01))]);
    let latch_at = if open { seconds - 0.03 } else { 0.0 };
    finish(room(&mix(vec![(0.0, 1.0, air), (latch_at, 0.8, latch)]), 0.15, 0.5, 0.06), QUIET)
}

/// A card set in its place: a low thunk and a latch. `step` moves the
/// pitch, so three of them are three installs; `twice` is the deck
/// builder's, which latches a second time.
fn lock_in(step: i32, twice: bool) -> Buf {
    let mut rng = Rng(31u32.wrapping_add_signed(step));
    let weight = thump(0.12, note(step - 31), note(step - 41), 0.04);
    let clack = struck(filter(&noise(0.03, &mut rng), Pass::Low, 1.2, |_| 1400.0), 0.0005, 0.007);
    let latch = metal(0.07, note(step - 14), 2.5, 0.016);
    let mut layers = vec![(0.0, 1.0, weight), (0.0, 0.6, clack), (0.004, 0.45, latch)];
    if twice {
        layers.push((0.07, 0.5, metal(0.07, note(step - 9), 2.5, 0.016)));
        layers.push((0.07, 0.4, thump(0.05, 110.0, 60.0, 0.012)));
    }
    finish(mix(layers), TABLE)
}

/// A card arriving in a hand: a chitter of data, grains of filtered
/// noise with no pitch to follow.
fn data_in(seed: u32) -> Buf {
    let mut rng = Rng(seed);
    let grains = (0..5)
        .map(|i| {
            let centre = 900.0 + 1700.0 * rng.unit();
            let grain = struck(filter(&noise(0.012, &mut rng), Pass::Band, 6.0, |_| centre), 0.0005, 0.004);
            (i as f32 * 0.013 + 0.004 * rng.unit(), 1.0 - 0.12 * i as f32, grain)
        })
        .collect();
    let under = thump(0.05, 130.0, 80.0, 0.012);
    finish(mix(vec![(0.0, 1.0, crush(&mix(grains), 20.0, |_| 4.0)), (0.0, 0.5, under)]), TABLE * 0.7)
}

/// Credits taken: a transfer — the counter running up, a fast roll of
/// ticks, and the tone that says it cleared. (The first try was one
/// struck contact, and the person did not hear a transfer in it.)
/// `seed` moves the roll, so three of them are three transfers.
fn credit(seed: u32) -> Buf {
    let mut rng = Rng(seed);
    let count = 7;
    let roll = (0..count)
        .map(|i| {
            let centre = 2100.0 + 500.0 * rng.unit();
            (i as f32 * 0.011, 0.5 + 0.5 * i as f32 / count as f32, ping(0.012, centre, 7.0, &mut rng))
        })
        .collect();
    let cleared_at = count as f32 * 0.011 + 0.006;
    let pitch = 1245.0;
    let cleared = struck(filter(&mix(vec![(0.0, 1.0, tone(Wave::Sine, 0.12, 0.0, &|_| pitch)), (0.0, 0.5, tone(Wave::Sine, 0.12, 0.0, &|_| pitch * 1.5))]), Pass::Low, 1.0, |_| 3200.0), 0.003, 0.03);
    finish(room(&mix(vec![(0.0, 0.8, mix(roll)), (cleared_at, 1.0, cleared)]), 0.15, 0.5, 0.08), TABLE * 0.7)
}

/// A game begins: a machine switched on — the breaker, the charge
/// climbing, the drives chattering, and the tone that says it is up.
/// (The first try was a drone swelling into a thump, which the person
/// said did not feel right at all: it was a mood, and this is a
/// sequence of things happening.)
fn boot() -> Buf {
    let mut rng = Rng(101);
    let breaker = mix(vec![(0.0, 1.0, thump(0.25, 110.0, 45.0, 0.06)), (0.0, 0.7, metal(0.12, 170.0, 3.5, 0.03)), (0.0, 0.5, struck(filter(&noise(0.03, &mut rng), Pass::Low, 1.0, |_| 2000.0), 0.0005, 0.008))]);
    let charge_time = 0.7;
    let whine = shape(filter(&tone(Wave::Saw, charge_time, 0.0, &glide(180.0, 2400.0, charge_time)), Pass::Low, 2.0, glide(500.0, 4500.0, charge_time)), |t, length| (t / length).powi(2) * (1.0 - (t / length).powi(8)));
    let turbine = shape(filter(&supersaw(charge_time, 24.0, &glide(note(-36), note(-24), charge_time)), Pass::Low, 2.5, glide(150.0, 1400.0, charge_time)), |t, length| (t / length).powf(1.5));
    let drives = (0..22)
        .map(|i| {
            let centre = 700.0 + 2600.0 * rng.unit();
            (i as f32 * 0.021 + 0.012 * rng.unit(), 0.4 + 0.6 * rng.unit(), ping(0.02, centre, 8.0, &mut rng))
        })
        .collect();
    let up_time = 0.7;
    let up = shape(drone(&[-24, -17, -12], up_time, 0.0, |t| 500.0 + 2600.0 * (-t / 0.25).exp()), |t, length| (t / 0.015).min(1.0) * (1.0 - t / length).powf(1.3));
    let landed = thump(0.4, 85.0, 40.0, 0.12);
    finish(
        room(&mix(vec![(0.0, 0.9, breaker), (0.1, 0.22, whine), (0.1, 0.7, turbine), (0.45, 0.35, mix(drives)), (0.1 + charge_time, 0.9, up), (0.1 + charge_time, 0.8, landed)]), 0.28, 0.72, 0.4),
        EVENT,
    )
}

/// A run begins: the line comes up out of the floor and lands.
fn jack_in() -> Buf {
    let mut rng = Rng(211);
    let rise_time = 0.45;
    let rise = filter(&supersaw(rise_time, 26.0, &glide(note(-36), note(-17), rise_time)), Pass::Low, 3.5, glide(160.0, 2600.0, rise_time));
    let rise = shape(rise, |t, length| (t / length).powf(1.6));
    let air = shape(whoosh(rise_time, 300.0, 3000.0, 1.5, &mut rng), |t, length| t / length);
    let carrier = shape(crush(&filter(&chatter(rise_time, &mut rng, 800.0, 1300.0), Pass::Band, 2.0, |_| 1000.0), 10.0, |_| 6.0), |t, length| 0.5 * t / length);
    let hit = thump(0.5, 90.0, 34.0, 0.15);
    let slam = struck(filter(&noise(0.2, &mut rng), Pass::Low, 1.0, glide(2200.0, 300.0, 0.15)), 0.001, 0.04);
    finish(room(&mix(vec![(0.0, 0.8, rise), (0.0, 0.35, air), (0.0, 0.3, carrier), (rise_time, 1.0, hit), (rise_time, 0.5, slam)]), 0.28, 0.72, 0.4), EVENT)
}

/// The Runner pulls the plug: the line falls and the carrier drops.
fn jack_out() -> Buf {
    let mut rng = Rng(223);
    let fall_time = 0.32;
    let fall = filter(&supersaw(fall_time, 26.0, &glide(note(-17), note(-41), fall_time)), Pass::Low, 3.0, glide(2400.0, 140.0, fall_time));
    let fall = shape(fall, |t, length| 1.0 - 0.6 * t / length);
    let hit = thump(0.25, 70.0, 34.0, 0.07);
    let static_ = shape(filter(&crackle(0.3, &mut rng, 160.0), Pass::Band, 2.0, |_| 1500.0), |t, length| (1.0 - t / length).powi(2));
    finish(room(&mix(vec![(0.0, 0.9, fall), (fall_time - 0.02, 0.9, hit), (fall_time, 1.6, static_)]), 0.22, 0.6, 0.2), EVENT * 0.9)
}

/// ICE ahead: a sonar pulse and its returns. The pulse sits in the
/// middle of the range with a tick on its front: the first try was a
/// 196 Hz sine over a sub, behind a 1.2 kHz filter, and on the person's
/// speakers it was silence — a sound whose every part is under 200 Hz
/// is one small speakers do not make.
fn ice_approach() -> Buf {
    let mut rng = Rng(801);
    let body = struck(filter(&supersaw(0.35, 8.0, &glide(392.0, 370.0, 0.35)), Pass::Low, 2.5, glide(2400.0, 700.0, 0.2)), 0.005, 0.09);
    let front = ping(0.08, 1150.0, 9.0, &mut rng);
    let floor = thump(0.2, 98.0, 55.0, 0.06);
    let pulse = mix(vec![(0.0, 1.0, body), (0.0, 0.5, front), (0.0, 0.6, floor)]);
    finish(room(&echo(&pulse, 0.14, 0.45, 0.45), 0.32, 0.72, 0.25), EVENT * 0.75)
}

/// The ICE answers: a hard, gated stab on a tritone.
fn ice_encounter() -> Buf {
    let seconds = 0.26;
    let chord = mix(vec![
        (0.0, 1.0, supersaw(seconds, 18.0, &|_| note(-29))),
        (0.0, 0.8, supersaw(seconds, 18.0, &|_| note(-23))),
        (0.0, 0.7, tone(Wave::Sine, seconds, 0.0, &|_| note(-41))),
    ]);
    let gated = shape(filter(&chord, Pass::Low, 3.0, glide(2000.0, 500.0, seconds)), |t, length| if (t * 28.0).fract() < 0.6 { 1.0 - 0.5 * t / length } else { 0.0 });
    let bite = crush(&drive(gated, 3.0), 16.0, |_| 3.0);
    finish(room(&bite, 0.2, 0.6, 0.18), EVENT)
}

/// The run got through: the lock gives — a latch, and a low fifth
/// opening behind it.
fn run_success() -> Buf {
    let seconds = 0.5;
    let granted = shape(drone(&[-29, -22, -17], seconds, 0.0, glide(250.0, 2200.0, 0.22)), |t, length| (t / 0.05).min(1.0) * (1.0 - t / length).powf(1.5));
    let latch = mix(vec![(0.0, 1.0, metal(0.12, 196.0, 3.0, 0.03)), (0.0, 0.8, thump(0.2, 80.0, 45.0, 0.06))]);
    finish(room(&mix(vec![(0.0, 1.0, latch), (0.03, 0.8, granted)]), 0.28, 0.72, 0.3), EVENT * 0.85)
}

/// The run was ended: the power goes, and the converter with it.
fn run_ended() -> Buf {
    let seconds = 0.5;
    let fall = filter(&supersaw(seconds, 25.0, &glide(note(-21), note(-48), seconds)), Pass::Low, 2.5, glide(1800.0, 110.0, seconds));
    let dying = crush(&fall, 20.0, |t| 1.0 + 60.0 * (t / seconds).powi(2));
    let slam = mix(vec![(0.0, 1.0, struck(filter(&noise(0.06, &mut Rng(241)), Pass::Low, 1.0, |_| 1200.0), 0.0005, 0.014)), (0.0, 1.0, thump(0.2, 85.0, 40.0, 0.05))]);
    finish(room(&mix(vec![(0.0, 1.0, shape(dying, |t, length| 1.0 - (t / length).powi(2))), (0.0, 0.8, slam)]), 0.18, 0.55, 0.15), EVENT * 0.9)
}

/// A card turned faceup: a program coming alive — it stutters, the
/// stutter quickens until it is a tone, the tone opens, and it runs.
/// (The first try was power coming on and a heavy part engaging: a
/// machine, where the person wanted something waking.)
fn rez() -> Buf {
    let mut rng = Rng(601);
    let wake = 0.42;
    let voice = filter(&supersaw(wake, 16.0, &glide(note(-24), note(-12), wake)), Pass::Low, 3.5, glide(300.0, 4200.0, wake));
    // The gate's rate climbs from a stutter to a blur: 9 Hz to 60 Hz.
    let mut phase = 0.0f32;
    let stutter: Buf = voice
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            let progress = i as f32 / (wake * SR);
            phase = (phase + (9.0 + 51.0 * progress * progress) / SR).fract();
            let open = if phase < 0.55 { 1.0 } else { 0.15 + 0.85 * progress.powi(3) };
            x * open * (0.3 + 0.7 * progress)
        })
        .collect();
    let thoughts = (0..9)
        .map(|i| {
            let at = wake * (i as f32 / 9.0).sqrt();
            let centre = 1400.0 + 2200.0 * rng.unit();
            (at, 0.5 + 0.5 * i as f32 / 9.0, ping(0.02, centre, 8.0, &mut rng))
        })
        .collect();
    let alive_time = 0.4;
    let alive = shape(drone(&[-12, -5, 0], alive_time, 0.0, |t| 1200.0 + 3000.0 * (-t / 0.12).exp()), |t, length| (1.0 - t / length).powf(1.4) * (1.0 + 0.25 * (TAU * 14.0 * t).sin()));
    finish(room(&mix(vec![(0.0, 0.9, stutter), (0.0, 0.3, mix(thoughts)), (wake, 0.9, alive), (wake, 0.6, thump(0.2, 90.0, 50.0, 0.05))]), 0.26, 0.7, 0.3), EVENT * 0.8)
}

/// Damage, of every kind — net, meat and core: an electroshock. The
/// snap of the arc, the buzz while it holds, the body's jolt under it,
/// and the sparks after. (This was the tag's sound for one render; the
/// person moved it here, where a thing lands on the Runner, in place of
/// a tearing signal.)
fn damage() -> Buf {
    let mut rng = Rng(701);
    let snap = struck(filter(&noise(0.02, &mut rng), Pass::Band, 0.8, |_| 3500.0), 0.0002, 0.004);
    let hold = 0.26;
    // Mains hum driven square, torn by noise at twice its rate.
    let buzz = drive(tone(Wave::Saw, hold, 0.0, &|_| 104.0), 6.0);
    let arc = filter(&noise(hold, &mut rng), Pass::Band, 1.5, |t| 2400.0 + 1200.0 * (TAU * 37.0 * t).sin());
    let live: Buf = buzz
        .iter()
        .zip(&arc)
        .enumerate()
        .map(|(i, (hum, air))| {
            let t = i as f32 / SR;
            let pulse = (TAU * 104.0 * t).sin().abs().powi(3);
            (0.5 * hum + 1.5 * air * pulse) * (1.0 - 0.4 * t / hold)
        })
        .collect();
    let live = crush(&live, 12.0, |_| 3.0);
    let jolt = thump(0.2, 150.0, 50.0, 0.05);
    let sparks = shape(filter(&crackle(0.25, &mut rng, 260.0), Pass::Band, 1.5, |_| 3000.0), |t, length| (1.0 - t / length).powi(2));
    finish(room(&mix(vec![(0.0, 1.2, snap), (0.0, 0.8, jolt), (0.004, 1.0, live), (hold, 1.8, sparks)]), 0.16, 0.55, 0.15), EVENT)
}

/// A tag: something applied to the Runner — the tracker swings in,
/// clamps on, and its beacon starts. (A klaxon first, then a shock;
/// the person asked for the sound of a thing being put on, and the
/// shock went to damage.)
fn tag() -> Buf {
    let mut rng = Rng(709);
    let reach = whoosh(0.09, 2600.0, 500.0, 3.0, &mut rng);
    let clamp = mix(vec![
        (0.0, 1.0, metal(0.1, 310.0, 3.5, 0.02)),
        (0.0, 0.9, thump(0.1, 140.0, 70.0, 0.025)),
        (0.0, 0.6, struck(filter(&noise(0.02, &mut rng), Pass::Low, 1.0, |_| 2500.0), 0.0003, 0.005)),
    ]);
    let pip = || struck(filter(&tone(Wave::Sine, 0.06, 0.0, &|_| 1480.0), Pass::Low, 1.0, |_| 3000.0), 0.002, 0.018);
    finish(room(&mix(vec![(0.0, 0.5, reach), (0.08, 1.0, clamp), (0.2, 0.45, pip()), (0.3, 0.45, pip())]), 0.2, 0.6, 0.15), EVENT * 0.75)
}

/// A card advanced: power committed — a heavy notch engaging, and a
/// pulse of current at one pitch that swells and is gone. **Nothing in
/// it rises**: the first try pulled a saw up a fifth behind the notch,
/// and the person heard a frog jumping. A short upward glide is a jump,
/// whatever it is made of.
fn advance() -> Buf {
    let mut rng = Rng(811);
    let notch = mix(vec![
        (0.0, 1.0, thump(0.12, 120.0, 55.0, 0.035)),
        (0.0, 0.8, metal(0.08, 230.0, 3.5, 0.016)),
        (0.0, 0.5, struck(filter(&noise(0.025, &mut rng), Pass::Low, 1.0, |_| 1800.0), 0.0004, 0.006)),
    ]);
    let seconds = 0.16;
    let current = drone(&[-24, -17], seconds, 0.0, |t| 400.0 + 1500.0 * (PI * t / seconds).sin());
    let current = shape(drive(current, 2.5), |t, length| (PI * t / length).sin() * (0.75 + 0.25 * (TAU * 52.0 * t).sin()));
    finish(room(&mix(vec![(0.0, 1.0, notch), (0.015, 0.6, current)]), 0.2, 0.6, 0.15), TABLE)
}

/// An agenda scored: the Corp getting what it wanted — two hits, the
/// second higher and held, root, fifth and octave behind a filter that
/// opens. (The first try was one low chord under a closing filter, and
/// the person heard it as sad: a filter closing over a low chord is a
/// thing ending. This one rises and opens.)
fn score() -> Buf {
    let first = struck(drone(&[-26, -19, -14], 0.16, 0.0, |_| 2200.0), 0.003, 0.05);
    let held_time = 0.7;
    let held = drone(&[-19, -12, -7, 0], held_time, 0.0, |t| 900.0 + 3600.0 * (t / 0.12).min(1.0) * (-((t - 0.12).max(0.0)) / 0.5).exp());
    let held = shape(held, |t, length| (t / 0.004).min(1.0) * (1.0 - (t / length).powi(2)));
    let hit = |seconds: f32| mix(vec![(0.0, 1.0, thump(seconds, 100.0, 45.0, seconds * 0.3)), (0.0, 0.4, metal(seconds, 196.0, 2.5, seconds * 0.2))]);
    finish(room(&mix(vec![(0.0, 0.8, first), (0.0, 0.8, hit(0.15)), (0.15, 1.0, held), (0.15, 0.9, hit(0.45))]), 0.3, 0.76, 0.45), EVENT)
}

/// An agenda stolen: the same weight, wrenched out of place and
/// breaking up.
fn steal() -> Buf {
    let seconds = 0.8;
    let chord = struck(drone(&[-31, -25, -19], seconds, -5.0, glide(2600.0, 350.0, seconds * 0.6)), 0.004, 0.3);
    let glitched = crush(&chord, 14.0, |t| 1.0 + 18.0 * ((t * 31.0).sin() * 0.5 + 0.5));
    let broken = shape(glitched, |t, _| if t > 0.15 && (t * 19.0).fract() < 0.25 { 0.1 } else { 1.0 });
    let rip = struck(filter(&crackle(0.3, &mut Rng(331), 400.0), Pass::Band, 2.0, |_| 1800.0), 0.001, 0.1);
    finish(room(&mix(vec![(0.0, 1.0, broken), (0.0, 0.8, thump(0.4, 90.0, 36.0, 0.12)), (0.0, 0.9, rip)]), 0.3, 0.74, 0.4), EVENT)
}

/// The turn passes: one low swell, a fifth, there and gone.
fn turn_start() -> Buf {
    let seconds = 0.45;
    let swell = hump(drone(&[-24, -17], seconds, 0.0, |t| 300.0 + 900.0 * (PI * t / seconds).sin()));
    let tick = ping(0.05, 520.0, 6.0, &mut Rng(401));
    finish(room(&mix(vec![(0.0, 1.0, swell), (0.0, 0.25, tick)]), 0.3, 0.72, 0.3), TABLE * 0.75)
}

/// The match won: a long swell that opens and holds — root, fifth,
/// octave and the fifth above, no third — over a sub that arrives with
/// it.
fn win() -> Buf {
    let mut rng = Rng(501);
    let seconds = 2.0;
    let swell = drone(&[-31, -24, -19, -12], seconds, 0.0, |t| 180.0 + 2400.0 * (t / 0.9).min(1.0).powi(2) * (1.0 - 0.5 * ((t - 0.9).max(0.0) / 1.1)));
    let swell = shape(swell, |t, length| (t / 0.7).min(1.0).powf(1.5) * (1.0 - ((t - 1.2).max(0.0) / (length - 1.2)).powi(2)));
    let arrive = mix(vec![(0.0, 1.0, thump(0.9, 75.0, 36.0, 0.3)), (0.0, 0.3, metal(0.5, 147.0, 2.5, 0.14))]);
    let air = shape(whoosh(0.8, 300.0, 4000.0, 1.2, &mut rng), |t, length| (t / length).powi(2));
    finish(room(&mix(vec![(0.0, 1.0, swell), (0.0, 0.25, air), (0.75, 0.9, arrive)]), 0.36, 0.82, 0.6), EVENT)
}

/// The match lost: the same voice sinking — bent down, the filter
/// closing over it, the converter failing, and the line going dead.
fn loss() -> Buf {
    let mut rng = Rng(503);
    let seconds = 1.7;
    let sink = drone(&[-31, -25, -19], seconds, -9.0, glide(1700.0, 90.0, seconds));
    let failing = crush(&sink, 18.0, |t| 1.0 + 70.0 * (t / seconds).powi(3));
    let sinking = shape(failing, |t, length| (t / 0.02).min(1.0) * (1.0 - (t / length).powi(2)));
    let hit = thump(0.6, 85.0, 30.0, 0.2);
    let static_ = shape(filter(&crackle(seconds, &mut rng, 90.0), Pass::Band, 2.0, |_| 1300.0), |t, length| (t / length) * (1.0 - (t / length).powi(4)));
    let dead = thump(0.4, 60.0, 28.0, 0.1);
    finish(room(&mix(vec![(0.0, 1.0, sinking), (0.0, 0.9, hit), (0.0, 1.2, static_), (seconds - 0.1, 0.9, dead)]), 0.34, 0.8, 0.5), EVENT)
}

/// Every file in `assets/sfx/`, by its name without the extension.
fn sounds() -> Vec<(&'static str, Buf)> {
    vec![
        ("boot", boot()),
        ("data-in-1", data_in(41)),
        ("data-in-2", data_in(43)),
        ("data-in-3", data_in(47)),
        ("lock-in-1", lock_in(0, false)),
        ("lock-in-2", lock_in(-2, false)),
        ("lock-in-3", lock_in(3, false)),
        ("credit-1", credit(53)),
        ("credit-2", credit(59)),
        ("credit-3", credit(61)),
        ("click", click()),
        ("toggle", toggle()),
        ("switch", sweep(true)),
        ("back", sweep(false)),
        ("deck-add", lock_in(0, true)),
        ("panel-open", panel(true)),
        ("panel-close", panel(false)),
        ("jack-in", jack_in()),
        ("jack-out", jack_out()),
        ("ice-approach", ice_approach()),
        ("ice-encounter", ice_encounter()),
        ("run-success", run_success()),
        ("run-ended", run_ended()),
        ("rez", rez()),
        ("damage", damage()),
        ("tag", tag()),
        ("advance", advance()),
        ("score", score()),
        ("steal", steal()),
        ("turn-start", turn_start()),
        ("win", win()),
        ("loss", loss()),
    ]
}

/// 16-bit mono PCM, the header written by hand as `audio::synth` writes
/// its own.
fn wav(samples: &[f32]) -> Vec<u8> {
    let rate = SR as u32;
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16).to_le_bytes());
    }
    out
}

fn main() -> std::io::Result<()> {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: render_sfx <output directory>");
        std::process::exit(2);
    };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir)?;
    for (name, mut samples) in sounds() {
        // VLC's encoder, the one this machine has, drops its last buffer
        // (and writes nothing at all for a 30 ms file); what it drops is
        // this.
        samples.resize(samples.len() + n(TRAILING_SILENCE), 0.0);
        std::fs::write(dir.join(format!("{name}.wav")), wav(&samples))?;
        println!("{name}.wav  {:.2} s", samples.len() as f32 / SR);
    }
    Ok(())
}
