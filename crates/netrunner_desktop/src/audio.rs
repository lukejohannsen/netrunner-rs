//! The sound bank and the music: what plays, when, and how loud.
//!
//! **Sound effects** are asked for by a [`PlaySfx`] message, which any
//! system may write, or by a [`ButtonSound`] on a button, which plays when
//! the button is pressed. Which moment makes which sound is
//! `models::sound`'s; this module only plays them. Each [`Sfx`] is a
//! *set* of recordings, one drawn at random per play, read from `sfx/`
//! across the asset tiers (`assets::list_files`): every `.ogg` whose name
//! is one of the set's stems, alone or followed by `-<n>` or `_<n>`. So a
//! player adds a variant by dropping `lock-in-4.ogg` into their own
//! `sfx/`, and replaces one by dropping a file of the same name.
//!
//! **The shipped recordings are made in this project**, by
//! `examples/render_sfx.rs` (`scripts/render_sfx.sh`): a cyberpunk set
//! the person asked for on 4 October 2026, in place of Kenney's recorded
//! paper cards and poker chips. **A set with no file is synthesized
//! here** (`synth`): a burst of shaped noise or a short chirp, written
//! as a WAV in memory. That is the tier that always works (AGENTS.md §5)
//! and it is deliberately plain, so a missing file is noticed rather
//! than mistaken for the look.
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
use crate::widgets::dropdown::Dropdown;
use crate::widgets::reader::Reading;
use crate::widgets::Pressed;

pub use crate::models::sound::Sfx;
use crate::models::sound::opened_or_closed;

/// The menus' theme, by its file's stem in `music/`.
pub const MENU_THEME: &str = "glass-and-morning-sky";

/// How long a track takes to fade in, and the one it replaces to fade out.
const FADE_IN: Duration = Duration::from_millis(1500);
const FADE_OUT: Duration = Duration::from_millis(900);

/// Two interface sounds closer than this are one press heard twice: a
/// Back button plays Back, and the navigation it writes would play it
/// again a frame later.
const UI_DEBOUNCE: Duration = Duration::from_millis(120);

/// How long a plain click waits for what the press did. A press that
/// opens a sheet or changes the screen is heard as that, and the sound
/// of it is written a frame or two after the press — by the navigation,
/// or by the game's model once it has taken the intent — so the click
/// is held this long and dropped if a named interface sound got there
/// first. Under the ear's threshold for "late".
const CLICK_WAIT: Duration = Duration::from_millis(35);

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
            .add_systems(Update, (button_sounds, navigation_sounds, popup_sounds, play, music).chain().after(nav::apply_navigation));
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

/// The file stems that are `sfx`'s recordings in `sfx/`, each alone or
/// with a number. `assets/sfx/README.md` lists them, and a test holds it
/// to this.
fn stem(sfx: Sfx) -> &'static str {
    match sfx {
        Sfx::Opening => "boot",
        Sfx::Place => "lock-in",
        Sfx::Take => "data-in",
        Sfx::Chips => "credit",
        Sfx::Click => "click",
        Sfx::Toggle => "toggle",
        Sfx::Switch => "switch",
        Sfx::Back => "back",
        Sfx::DeckAdd => "deck-add",
        Sfx::Open => "panel-open",
        Sfx::Close => "panel-close",
        Sfx::JackIn => "jack-in",
        Sfx::JackOut => "jack-out",
        Sfx::Approach => "ice-approach",
        Sfx::Encounter => "ice-encounter",
        Sfx::RunSuccess => "run-success",
        Sfx::RunEnded => "run-ended",
        Sfx::Rez => "rez",
        Sfx::Damage => "damage",
        Sfx::Tag => "tag",
        Sfx::Advance => "advance",
        Sfx::Score => "score",
        Sfx::Steal => "steal",
        Sfx::TurnStart => "turn-start",
        Sfx::Win => "win",
        Sfx::Loss => "loss",
    }
}

/// The interface's own sounds, which [`UI_DEBOUNCE`] applies to.
fn is_interface(sfx: Sfx) -> bool {
    matches!(sfx, Sfx::Click | Sfx::Toggle | Sfx::Switch | Sfx::Back | Sfx::Open | Sfx::Close)
}

/// Whether `file` (`lock-in-3.ogg`) is one of `stem`'s recordings.
fn is_recording_of(file: &str, stem: &str) -> bool {
    let Some(name) = file.strip_suffix(".ogg").or_else(|| file.strip_suffix(".OGG")) else { return false };
    match name.strip_prefix(stem) {
        Some("") => true,
        Some(rest) => rest.strip_prefix(['-', '_']).is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())),
        None => false,
    }
}

/// Every set's recordings. Loaded on the first sound rather than at
/// start-up, so a client that never makes one never reads a file.
#[derive(Resource, Default)]
struct Bank {
    loaded: bool,
    sets: HashMap<Sfx, Vec<Handle<AudioSource>>>,
}

impl Bank {
    fn load(&mut self, sources: &mut Assets<AudioSource>) {
        self.loaded = true;
        let files = assets::list_files("sfx", "ogg");
        for sfx in Sfx::ALL {
            let mut recordings: Vec<Handle<AudioSource>> = files
                .iter()
                .filter(|file| is_recording_of(file, stem(sfx)))
                .filter_map(|file| assets::read(&format!("sfx/{file}")))
                .map(|bytes| sources.add(AudioSource { bytes: Arc::from(bytes) }))
                .collect();
            if recordings.is_empty() {
                recordings.push(sources.add(AudioSource { bytes: Arc::from(synth::wav(sfx)) }));
            }
            self.sets.insert(sfx, recordings);
        }
    }
}

/// The sounds waiting for their moment.
#[derive(Resource, Default)]
struct Queue {
    pending: Vec<(Duration, Sfx)>,
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

/// A press is heard: as the button's own sound when it names one, and
/// as a click otherwise, which waits [`CLICK_WAIT`] for the press to
/// turn out to be something with a sound of its own.
fn button_sounds(mut pressed: MessageReader<Pressed>, sounds: Query<&ButtonSound>, mut out: MessageWriter<PlaySfx>) {
    for Pressed(entity) in pressed.read() {
        out.write(match sounds.get(*entity) {
            Ok(ButtonSound(sfx)) => PlaySfx::now(*sfx),
            Err(_) => PlaySfx::after(Sfx::Click, CLICK_WAIT),
        });
    }
}

/// What opens over a menu screen: a drop-down's list and the card
/// reader. Both are widgets any screen may use, so the sound is read off
/// their state here rather than written at each place one is opened.
/// (The board's own sheets and menus are the game model's, which says
/// so itself: `models::game::Game::apply`.)
fn popup_sounds(dropdowns: Query<&Dropdown>, reading: Option<Res<Reading>>, mut was_open: Local<usize>, mut out: MessageWriter<PlaySfx>) {
    let open = dropdowns.iter().filter(|dropdown| dropdown.open).count() + usize::from(reading.is_some_and(|reading| reading.is_open()));
    if let Some(sfx) = opened_or_closed(*was_open, open) {
        out.write(PlaySfx::now(sfx));
    }
    *was_open = open;
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
    if to == here || matches!(here, AppScreen::Boot | AppScreen::Splash | AppScreen::FirstLaunch) {
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
        queue.pending.push((now + request.after, request.sfx));
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
    // A named interface sound due in the same frame as a click is what
    // the click was; it goes first, and the debounce then drops the click.
    let named = |sfx: Sfx| is_interface(sfx) && sfx != Sfx::Click;
    let (first, rest): (Vec<_>, Vec<_>) = due.into_iter().partition(|(_, sfx)| named(*sfx));
    for (_, sfx) in first.into_iter().chain(rest) {
        if is_interface(sfx) {
            if queue.last_interface.is_some_and(|last| now.saturating_sub(last) < UI_DEBOUNCE) {
                continue;
            }
            queue.last_interface = Some(now);
        }
        let Some(recordings) = bank.sets.get(&sfx).filter(|recordings| !recordings.is_empty()) else { continue };
        let handle = recordings[dice.below(recordings.len())].clone();
        commands.spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume))));
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
/// WAV, plain on purpose. The shipped sounds are not made here but by
/// `examples/render_sfx.rs`.
mod synth {
    use super::Sfx;

    const RATE: u32 = 44_100;

    /// `sfx`'s stand-in.
    pub fn wav(sfx: Sfx) -> Vec<u8> {
        let samples = match sfx {
            Sfx::Opening => chirp(300.0, 1200.0, 0.5),
            Sfx::Place => thud(0.07),
            Sfx::Take => chirp(900.0, 1800.0, 0.05),
            Sfx::Chips => pings(&[3100.0, 3400.0], 0.045),
            Sfx::Click => chirp(1500.0, 1500.0, 0.012),
            Sfx::Toggle => chirp(1800.0, 1800.0, 0.02),
            Sfx::Switch => chirp(900.0, 1400.0, 0.06),
            Sfx::Back => chirp(800.0, 400.0, 0.06),
            Sfx::DeckAdd => chirp(600.0, 1200.0, 0.05),
            Sfx::Open => chirp(400.0, 900.0, 0.1),
            Sfx::Close => chirp(900.0, 400.0, 0.1),
            Sfx::JackIn => chirp(150.0, 1200.0, 0.4),
            Sfx::JackOut => chirp(1200.0, 150.0, 0.3),
            Sfx::Approach => chirp(330.0, 330.0, 0.2),
            Sfx::Encounter => chirp(160.0, 140.0, 0.2),
            Sfx::RunSuccess => pings(&[880.0, 1108.0, 1318.0], 0.06),
            Sfx::RunEnded => chirp(400.0, 60.0, 0.4),
            Sfx::Rez => chirp(200.0, 800.0, 0.25),
            Sfx::Damage => thud(0.25),
            Sfx::Tag => pings(&[1000.0, 750.0, 1000.0, 750.0], 0.08),
            Sfx::Advance => chirp(300.0, 500.0, 0.08),
            Sfx::Score => pings(&[587.0, 740.0, 880.0], 0.08),
            Sfx::Steal => pings(&[587.0, 698.0, 830.0], 0.08),
            Sfx::TurnStart => pings(&[660.0, 880.0], 0.1),
            Sfx::Win => pings(&[587.0, 740.0, 880.0, 1174.0], 0.12),
            Sfx::Loss => pings(&[880.0, 698.0, 587.0, 440.0], 0.12),
        };
        encode(&samples)
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

    /// Low-passed noise that dies at once.
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

    /// Short bright tones one after another.
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
        assert!(is_recording_of("lock-in-3.ogg", "lock-in"));
        assert!(is_recording_of("toggle.ogg", "toggle"));
        assert!(is_recording_of("switch_001.ogg", "switch"));
        assert!(!is_recording_of("lock-inside.ogg", "lock-in"));
        assert!(!is_recording_of("lock-in-.ogg", "lock-in"));
        assert!(!is_recording_of("lock-in-3.wav", "lock-in"));
        // One stem is never the start of another's file.
        for a in Sfx::ALL {
            for b in Sfx::ALL {
                assert!(a == b || !is_recording_of(&format!("{}.ogg", stem(a)), stem(b)), "{a:?} / {b:?}");
            }
        }
    }

    /// Every sound ships as a file, every file is some sound's, and the
    /// guide beside them lists every stem: a new `Sfx` needs its
    /// recording and its row, and a recipe renamed in
    /// `examples/render_sfx.rs` cannot leave a file nothing plays.
    #[test]
    fn every_sound_has_a_bundled_recording_a_row_in_the_guide_and_no_stray_file() {
        // The committed folder itself, not the tiers: a player's own
        // `sfx/` may hold anything.
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/sfx");
        let mut files: Vec<String> = std::fs::read_dir(&folder).unwrap().filter_map(|entry| entry.ok()?.file_name().into_string().ok()).filter(|name| name.ends_with(".ogg")).collect();
        files.sort();
        let guide = std::fs::read_to_string(folder.join("README.md")).expect("the guide is committed");
        for sfx in Sfx::ALL {
            assert!(files.iter().any(|file| is_recording_of(file, stem(sfx))), "{sfx:?} has no recording in sfx/");
            assert!(guide.contains(&format!("`{}`", stem(sfx))), "{sfx:?}'s stem `{}` is not in assets/sfx/README.md", stem(sfx));
        }
        for file in &files {
            assert!(Sfx::ALL.iter().any(|sfx| is_recording_of(file, stem(*sfx))), "sfx/{file} is no sound's recording");
        }
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
    /// check that the features in `Cargo.toml` cover what is shipped —
    /// and, since the sounds are encoded by whichever encoder the machine
    /// that rendered them had, that the encoder wrote something.
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
        for sfx in Sfx::ALL {
            assert!(decodes(synth::wav(sfx)), "{sfx:?}'s stand-in");
        }
    }

    #[test]
    fn the_drawn_tier_is_a_wav_with_something_in_it() {
        for sfx in Sfx::ALL {
            let bytes = synth::wav(sfx);
            assert_eq!(&bytes[..4], b"RIFF");
            assert!(bytes.len() > 44, "{sfx:?}");
        }
    }
}
