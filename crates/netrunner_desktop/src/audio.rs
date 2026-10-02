//! The sound bank and the music: what plays, when, and how loud.
//!
//! **Sound effects** are asked for by a [`PlaySfx`] message, which any
//! system may write, or by a [`ButtonSound`] on a button, which plays when
//! the button is pressed. Which moment makes which sound is
//! `models::sound`'s; this module only plays them. Each [`Sfx`] is a
//! *set* of recordings, one drawn at random per play, read from `sfx/`
//! across the asset tiers (`assets::list_files`): every `.ogg` whose name
//! is one of the set's stems, alone or followed by `-<n>` or `_<n>`. So a
//! player adds a variant by dropping `card-place-5.ogg` into their own
//! `sfx/`, and replaces one by dropping a file of the same name.
//!
//! **A set nobody recorded is synthesized** ([`synth`]): a burst of shaped
//! noise or a short chirp, written as a WAV in memory. That is the tier
//! that always works (AGENTS.md §5) and it is deliberately plain, so a
//! missing file is noticed rather than mistaken for the look.
//!
//! **Music** is the files in `music/`. The menus loop one theme,
//! [`MENU_THEME`] (the person's choice, 1 October 2026); a game plays every
//! track, the theme included, in a random order that never repeats the
//! track just heard. A change of place fades the old track out and the
//! new one in rather than cutting. Music has no synthesized tier: a tune
//! made in code would be worse than the silence a missing folder leaves.
//!
//! **Both volumes are the settings file's** (`sfx_volume`, `music_volume`)
//! and are read every frame, so the settings row is heard as it is moved.
//! **A screenshot run is silent** (`NETRUNNER_SCREENSHOT`): it is driven by
//! nobody, usually on a virtual compositor, and its sound would come out
//! of the speakers of whoever is at the machine.
//!
//! Everything here asks for `Assets<AudioSource>` as an `Option`: the
//! headless tests build the client without Bevy's audio plugin, and there
//! the bank simply never loads.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::assets;
use crate::core::ClientCore;
use crate::dev::Dev;
use crate::nav::{self, Navigate};
use crate::screens::AppScreen;
use crate::widgets::Pressed;

pub use crate::models::sound::Sfx;

/// The menus' theme, by its file's stem in `music/`.
pub const MENU_THEME: &str = "glass-and-morning-sky";

/// How long a track takes to fade in, and the one it replaces to fade out.
const FADE_IN: Duration = Duration::from_millis(1500);
const FADE_OUT: Duration = Duration::from_millis(900);

/// Two interface sounds closer than this are one press heard twice: a
/// Back button plays Back, and the navigation it writes would play it
/// again a frame later.
const UI_DEBOUNCE: Duration = Duration::from_millis(120);

/// The gap between the sounds one action makes, so three cards drawn are
/// heard as three.
pub const STAGGER: Duration = Duration::from_millis(90);

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .init_resource::<Bank>()
            .init_resource::<Queue>()
            .init_resource::<Music>()
            .insert_resource(Dice::from_clock())
            .add_systems(OnEnter(AppScreen::Game), |mut out: MessageWriter<PlaySfx>| {
                out.write(PlaySfx::now(Sfx::Opening));
            })
            .add_systems(Update, (button_sounds, navigation_sounds, play, music).chain().after(nav::apply_navigation));
    }
}

/// Play `sfx`, `after` from now.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaySfx {
    pub sfx: Sfx,
    pub after: Duration,
}

impl PlaySfx {
    pub fn now(sfx: Sfx) -> Self {
        PlaySfx { sfx, after: Duration::ZERO }
    }

    pub fn after(sfx: Sfx, after: Duration) -> Self {
        PlaySfx { sfx, after }
    }
}

/// On a button: the sound its press makes. For a press whose sound is
/// not the press's but its result's — a card added to a deck only if the
/// deck took it — the screen writes [`PlaySfx`] itself.
#[derive(Component, Debug, Clone, Copy)]
pub struct ButtonSound(pub Sfx);

/// A set of recordings, as the bank keeps them: an [`Sfx`] is one set or
/// two, the opening being the shuffle and then the fan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Set {
    Shuffle,
    Fan,
    Place,
    Take,
    Chips,
    Toggle,
    Switch,
    Back,
    DeckAdd,
}

impl Set {
    const ALL: [Set; 9] = [Set::Shuffle, Set::Fan, Set::Place, Set::Take, Set::Chips, Set::Toggle, Set::Switch, Set::Back, Set::DeckAdd];

    /// The file stems that are this set's recordings, each alone or with
    /// a number: Kenney's names, kept so the register reads like the
    /// packs they came from.
    fn stems(self) -> &'static [&'static str] {
        match self {
            Set::Shuffle => &["card-shuffle"],
            Set::Fan => &["card-fan"],
            Set::Place => &["card-place"],
            Set::Take => &["card-shove", "card-slide"],
            Set::Chips => &["chips-stack"],
            Set::Toggle => &["toggle"],
            Set::Switch => &["switch"],
            Set::Back => &["back"],
            Set::DeckAdd => &["deck-add"],
        }
    }

    /// The interface's own sounds, which [`UI_DEBOUNCE`] applies to.
    fn is_interface(self) -> bool {
        matches!(self, Set::Toggle | Set::Switch | Set::Back)
    }
}

/// The set an [`Sfx`] starts with; the opening's fan follows its shuffle
/// in [`play`], once the shuffle drawn is over.
fn first_set(sfx: Sfx) -> Set {
    match sfx {
        Sfx::Opening => Set::Shuffle,
        Sfx::Place => Set::Place,
        Sfx::Take => Set::Take,
        Sfx::Chips => Set::Chips,
        Sfx::Toggle => Set::Toggle,
        Sfx::Switch => Set::Switch,
        Sfx::Back => Set::Back,
        Sfx::DeckAdd => Set::DeckAdd,
    }
}

/// Whether `file` (`card-slide-3.ogg`) is one of `stem`'s recordings.
fn is_recording_of(file: &str, stem: &str) -> bool {
    let Some(name) = file.strip_suffix(".ogg").or_else(|| file.strip_suffix(".OGG")) else { return false };
    match name.strip_prefix(stem) {
        Some("") => true,
        Some(rest) => rest.strip_prefix(['-', '_']).is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())),
        None => false,
    }
}

/// Every set's recordings, with each one's length (the opening needs the
/// shuffle's). Loaded on the first sound rather than at start-up, so a
/// client that never makes one never reads a file.
#[derive(Resource, Default)]
struct Bank {
    loaded: bool,
    sets: HashMap<Set, Vec<(Handle<AudioSource>, Duration)>>,
}

impl Bank {
    fn load(&mut self, sources: &mut Assets<AudioSource>) {
        self.loaded = true;
        let files = assets::list_files("sfx", "ogg");
        for set in Set::ALL {
            let mut recordings: Vec<(Handle<AudioSource>, Duration)> = files
                .iter()
                .filter(|file| set.stems().iter().any(|stem| is_recording_of(file, stem)))
                .filter_map(|file| assets::read(&format!("sfx/{file}")))
                .map(|bytes| {
                    let length = ogg_length(&bytes).unwrap_or(Duration::from_millis(500));
                    (sources.add(AudioSource { bytes: Arc::from(bytes) }), length)
                })
                .collect();
            if recordings.is_empty() {
                let (bytes, length) = synth::wav(set);
                recordings.push((sources.add(AudioSource { bytes: Arc::from(bytes) }), length));
            }
            self.sets.insert(set, recordings);
        }
    }
}

/// The length of an Ogg Vorbis file: the last page's granule position
/// (its sample count) over the identification header's sample rate.
fn ogg_length(bytes: &[u8]) -> Option<Duration> {
    let header = bytes.windows(7).position(|window| window == b"\x01vorbis")?;
    let rate = u32::from_le_bytes(bytes.get(header + 12..header + 16)?.try_into().ok()?);
    let last_page = bytes.windows(4).rposition(|window| window == b"OggS")?;
    let samples = i64::from_le_bytes(bytes.get(last_page + 6..last_page + 14)?.try_into().ok()?);
    (rate > 0 && samples > 0).then(|| Duration::from_secs_f64(samples as f64 / f64::from(rate)))
}

/// The sounds waiting for their moment.
#[derive(Resource, Default)]
struct Queue {
    pending: Vec<(Duration, Set)>,
    last_interface: Option<Duration>,
}

/// A small generator for the random picks — which recording, which
/// track. The wall clock seeds it, as it seeds the table's pick
/// (`screens::game::table_nonce`): the choice is cosmetic, and tying it
/// to the game's seed would make a replayed bug sound different from
/// itself for no reason anyone could use.
#[derive(Resource)]
struct Dice(u64);

impl Dice {
    fn from_clock() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos() as u64);
        Dice(nanos | 1)
    }

    fn below(&mut self, n: usize) -> usize {
        // xorshift64
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

fn silenced(dev: Option<&Dev>) -> bool {
    dev.is_some_and(|dev| dev.screenshot.is_some())
}

fn button_sounds(mut pressed: MessageReader<Pressed>, sounds: Query<&ButtonSound>, mut out: MessageWriter<PlaySfx>) {
    for Pressed(entity) in pressed.read() {
        if let Ok(ButtonSound(sfx)) = sounds.get(*entity) {
            out.write(PlaySfx::now(*sfx));
        }
    }
}

/// A screen left for another: Back when it is the screen this one goes
/// back to (`nav::back_from`, Escape's rule), a switch otherwise. Read
/// after `nav::apply_navigation`, in the frame it reads the same message,
/// so the screen being left is still the current one. The boot and the
/// splash change screens by themselves, and nobody selected that.
fn navigation_sounds(mut navigate: MessageReader<Navigate>, screen: Option<Res<State<AppScreen>>>, mut out: MessageWriter<PlaySfx>) {
    let last = navigate.read().last().copied();
    let (Some(Navigate(to)), Some(screen)) = (last, screen) else { return };
    let here = *screen.get();
    if to == here || matches!(here, AppScreen::Boot | AppScreen::Splash) {
        return;
    }
    out.write(PlaySfx::now(if nav::back_from(here) == Some(to) { Sfx::Back } else { Sfx::Switch }));
}

fn play(
    mut commands: Commands,
    mut requests: MessageReader<PlaySfx>,
    mut queue: ResMut<Queue>,
    mut bank: ResMut<Bank>,
    mut dice: ResMut<Dice>,
    sources: Option<ResMut<Assets<AudioSource>>>,
    time: Res<Time<Real>>,
    core: Option<Res<ClientCore>>,
    dev: Option<Res<Dev>>,
) {
    let now = time.elapsed();
    for request in requests.read() {
        queue.pending.push((now + request.after, first_set(request.sfx)));
    }
    let volume = core.map_or(0.0, |core| core.settings.desktop.sfx_volume);
    let Some(mut sources) = sources.filter(|_| volume > 0.0 && !silenced(dev.as_deref())) else {
        queue.pending.clear();
        return;
    };
    if queue.pending.iter().all(|(at, _)| *at > now) {
        return;
    }
    if !bank.loaded {
        bank.load(&mut sources);
    }
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut queue.pending).into_iter().partition(|(at, _)| *at <= now);
    queue.pending = later;
    for (_, set) in due {
        if set.is_interface() {
            if queue.last_interface.is_some_and(|last| now.saturating_sub(last) < UI_DEBOUNCE) {
                continue;
            }
            queue.last_interface = Some(now);
        }
        let Some(recordings) = bank.sets.get(&set).filter(|recordings| !recordings.is_empty()) else { continue };
        let (handle, length) = recordings[dice.below(recordings.len())].clone();
        commands.spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume))));
        // The deck is fanned once it is shuffled.
        if set == Set::Shuffle {
            queue.pending.push((now + length, Set::Fan));
        }
    }
}

/// Which music a place wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Menu,
    Game,
}

fn mode_of(screen: AppScreen) -> Option<Mode> {
    match screen {
        AppScreen::Boot => None,
        AppScreen::Game | AppScreen::Replay => Some(Mode::Game),
        _ => Some(Mode::Menu),
    }
}

/// The track playing, on its entity.
#[derive(Component)]
struct Track {
    mode: Mode,
    started: Duration,
}

/// A track on its way out.
#[derive(Component)]
struct FadingOut {
    from: f32,
    started: Duration,
}

/// The music folder's tracks, listed once, and the last one a game
/// played, which the next pick avoids.
#[derive(Resource, Default)]
struct Music {
    tracks: Option<Vec<String>>,
    last: Option<String>,
}

/// The track a place starts: the theme for the menus (or nothing, when
/// it is not installed — a menu's theme is a choice, and another track
/// in its place would not be it), and for a game any track but the last.
fn pick_track(mode: Mode, tracks: &[String], last: Option<&str>, roll: usize) -> Option<String> {
    match mode {
        Mode::Menu => tracks.iter().find(|track| track.strip_suffix(".ogg") == Some(MENU_THEME)).cloned(),
        Mode::Game => {
            let fresh: Vec<&String> = tracks.iter().filter(|track| Some(track.as_str()) != last).collect();
            let pool: Vec<&String> = if fresh.is_empty() { tracks.iter().collect() } else { fresh };
            pool.get(roll % pool.len().max(1)).map(|track| (*track).clone())
        }
    }
}

fn music(
    mut commands: Commands,
    mut music: ResMut<Music>,
    mut dice: ResMut<Dice>,
    mut playing: Query<(Entity, &Track, Option<&mut AudioSink>), Without<FadingOut>>,
    mut fading: Query<(Entity, &FadingOut, Option<&mut AudioSink>), Without<Track>>,
    sources: Option<ResMut<Assets<AudioSource>>>,
    screen: Option<Res<State<AppScreen>>>,
    time: Res<Time<Real>>,
    core: Option<Res<ClientCore>>,
    dev: Option<Res<Dev>>,
) {
    let now = time.elapsed();
    let volume = core.map_or(0.0, |core| core.settings.desktop.music_volume);
    let want = screen.and_then(|screen| mode_of(*screen.get())).filter(|_| volume > 0.0 && !silenced(dev.as_deref()) && sources.is_some());

    for (entity, fade, sink) in &mut fading {
        let left = 1.0 - now.saturating_sub(fade.started).as_secs_f32() / FADE_OUT.as_secs_f32();
        if left <= 0.0 {
            commands.entity(entity).despawn();
        } else if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(fade.from * left));
        }
    }

    let mut current = None;
    for (entity, track, sink) in &mut playing {
        let rise = (now.saturating_sub(track.started).as_secs_f32() / FADE_IN.as_secs_f32()).min(1.0);
        if want != Some(track.mode) {
            let from = sink.as_ref().map_or(0.0, |sink| sink.volume().to_linear());
            commands.entity(entity).remove::<Track>().insert(FadingOut { from, started: now });
            continue;
        }
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(volume * rise));
        }
        current = Some(track.mode);
    }

    let (Some(mode), None, Some(mut sources)) = (want, current, sources) else { return };
    let tracks = music.tracks.get_or_insert_with(|| assets::list_files("music", "ogg")).clone();
    let roll = dice.below(tracks.len().max(1));
    let Some(name) = pick_track(mode, &tracks, music.last.as_deref(), roll) else { return };
    let Some(bytes) = assets::read(&format!("music/{name}")) else {
        // Gone since it was listed: forget the list, so the next frame
        // reads the folder as it is now.
        music.tracks = None;
        return;
    };
    let playback = match mode {
        // The menus loop their theme; a game's track ends, and the frame
        // after it is gone, this system starts the next.
        Mode::Menu => PlaybackSettings::LOOP,
        Mode::Game => PlaybackSettings::DESPAWN,
    };
    if mode == Mode::Game {
        music.last = Some(name.clone());
    }
    commands.spawn((Name::new(format!("music {name}")), Track { mode, started: now }, AudioPlayer(sources.add(AudioSource { bytes: Arc::from(bytes) })), playback.with_volume(Volume::Linear(0.0))));
}

/// The tier that always works: each set drawn in code, as 16-bit mono
/// WAV, plain on purpose.
mod synth {
    use std::time::Duration;

    use super::Set;

    const RATE: u32 = 44_100;

    /// `set`'s stand-in and its length.
    pub fn wav(set: Set) -> (Vec<u8>, Duration) {
        let samples = match set {
            Set::Shuffle => riffle(1.2),
            Set::Fan => swish(0.35, 0.6),
            Set::Place => thud(0.07),
            Set::Take => swish(0.18, 0.8),
            Set::Chips => pings(&[3100.0, 2600.0, 3400.0], 0.045),
            Set::Toggle => chirp(1800.0, 1800.0, 0.02),
            Set::Switch => chirp(900.0, 1400.0, 0.06),
            Set::Back => chirp(800.0, 400.0, 0.06),
            Set::DeckAdd => chirp(600.0, 1200.0, 0.05),
        };
        let length = Duration::from_secs_f64(samples.len() as f64 / f64::from(RATE));
        (encode(&samples), length)
    }

    fn count(seconds: f32) -> usize {
        (seconds * RATE as f32) as usize
    }

    /// A fixed noise sequence, so the drawn tier sounds the same every run.
    fn noise(n: usize) -> Vec<f32> {
        let mut state: u32 = 0x9E37_79B9;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    /// Low-passed noise that dies at once: a card set down.
    fn thud(seconds: f32) -> Vec<f32> {
        let n = count(seconds);
        let mut low = 0.0;
        noise(n)
            .into_iter()
            .enumerate()
            .map(|(i, x)| {
                low += 0.15 * (x - low);
                low * 1.6 * (-(i as f32) / (n as f32 * 0.2)).exp()
            })
            .collect()
    }

    /// Noise that swells and falls: a card drawn across the felt.
    fn swish(seconds: f32, loudness: f32) -> Vec<f32> {
        let n = count(seconds);
        noise(n).into_iter().enumerate().map(|(i, x)| x * loudness * 0.4 * (std::f32::consts::PI * i as f32 / n as f32).sin()).collect()
    }

    /// Thuds in quick succession: a riffle.
    fn riffle(seconds: f32) -> Vec<f32> {
        let n = count(seconds);
        let flick = thud(0.03);
        let mut out = vec![0.0; n];
        let step = count(0.04);
        for start in (0..n).step_by(step) {
            for (offset, sample) in flick.iter().enumerate() {
                if let Some(slot) = out.get_mut(start + offset) {
                    *slot += sample * 0.6;
                }
            }
        }
        out
    }

    /// Short bright tones one after another: chips meeting.
    fn pings(pitches: &[f32], gap: f32) -> Vec<f32> {
        let ring = count(0.06);
        let step = count(gap);
        let mut out = vec![0.0; step * pitches.len() + ring];
        for (k, pitch) in pitches.iter().enumerate() {
            for i in 0..ring {
                let t = i as f32 / RATE as f32;
                out[k * step + i] += 0.3 * (std::f32::consts::TAU * pitch * t).sin() * (-t / 0.012).exp();
            }
        }
        out
    }

    /// A tone gliding from one pitch to another: a click with a direction.
    fn chirp(from: f32, to: f32, seconds: f32) -> Vec<f32> {
        let n = count(seconds);
        let mut phase = 0.0f32;
        (0..n)
            .map(|i| {
                let progress = i as f32 / n as f32;
                phase += std::f32::consts::TAU * (from + (to - from) * progress) / RATE as f32;
                0.3 * phase.sin() * (1.0 - progress)
            })
            .collect()
    }

    fn encode(samples: &[f32]) -> Vec<u8> {
        let data = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data.to_le_bytes());
        for sample in samples {
            out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_is_a_stem_alone_or_numbered() {
        assert!(is_recording_of("card-slide-3.ogg", "card-slide"));
        assert!(is_recording_of("toggle.ogg", "toggle"));
        assert!(is_recording_of("switch_001.ogg", "switch"));
        assert!(!is_recording_of("card-slider.ogg", "card-slide"));
        assert!(!is_recording_of("card-slide-.ogg", "card-slide"));
        assert!(!is_recording_of("card-slide-3.wav", "card-slide"));
    }

    #[test]
    fn every_set_has_a_bundled_recording_and_the_shuffle_has_a_length() {
        let files = assets::list_files("sfx", "ogg");
        for set in Set::ALL {
            assert!(files.iter().any(|file| set.stems().iter().any(|stem| is_recording_of(file, stem))), "{set:?} has no recording in sfx/");
        }
        let shuffle = assets::read("sfx/card-shuffle.ogg").expect("the shuffle is bundled");
        let length = ogg_length(&shuffle).expect("an Ogg Vorbis length");
        assert!(length > Duration::from_secs(2) && length < Duration::from_secs(4), "{length:?}");
    }

    #[test]
    fn the_menus_play_their_theme_and_a_game_never_repeats_the_last_track() {
        let tracks: Vec<String> = ["a.ogg", "glass-and-morning-sky.ogg", "b.ogg"].iter().map(|s| s.to_string()).collect();
        assert_eq!(pick_track(Mode::Menu, &tracks, None, 0).as_deref(), Some("glass-and-morning-sky.ogg"));
        assert_eq!(pick_track(Mode::Menu, &tracks[..1], None, 0), None, "no theme, no stand-in");
        for roll in 0..6 {
            assert_ne!(pick_track(Mode::Game, &tracks, Some("b.ogg"), roll).as_deref(), Some("b.ogg"));
        }
        assert_eq!(pick_track(Mode::Game, &tracks[..1], Some("a.ogg"), 3).as_deref(), Some("a.ogg"), "one track repeats");
        assert!(assets::list_files("music", "ogg").iter().any(|track| track.strip_suffix(".ogg") == Some(MENU_THEME)), "the theme is bundled");
    }

    /// Bevy decodes on the audio thread and unwraps a format it was not
    /// built for, so a file it cannot read is a panic at the moment it
    /// plays. Decoding every bundled file and every stand-in here is the
    /// check that the features in `Cargo.toml` cover what is shipped.
    #[test]
    fn every_bundled_sound_and_track_and_every_stand_in_decodes() {
        use bevy::audio::Decodable;
        let decodes = |bytes: Vec<u8>| AudioSource { bytes: Arc::from(bytes) }.decoder().take(4096).count() > 0;
        for file in assets::list_files("sfx", "ogg") {
            assert!(decodes(assets::read(&format!("sfx/{file}")).unwrap()), "sfx/{file}");
        }
        for track in assets::list_files("music", "ogg") {
            assert!(decodes(assets::read(&format!("music/{track}")).unwrap()), "music/{track}");
        }
        for set in Set::ALL {
            assert!(decodes(synth::wav(set).0), "{set:?}'s stand-in");
        }
    }

    #[test]
    fn the_drawn_tier_is_a_wav_of_the_length_it_claims() {
        for set in Set::ALL {
            let (bytes, length) = synth::wav(set);
            assert_eq!(&bytes[..4], b"RIFF");
            let samples = (bytes.len() - 44) / 2;
            assert_eq!(Duration::from_secs_f64(samples as f64 / 44_100.0), length, "{set:?}");
            assert!(length > Duration::ZERO);
        }
    }
}
