//! Procedural audio. The game ships no asset files: every cue is synthesized into 16-bit PCM
//! mono WAV bytes at startup and registered as an [`AudioSource`], so the whole soundtrack is a
//! few kilobytes of code.
//!
//! The lighthouse keeper hears the sea from the lantern room, so the spatial listener sits at the
//! top of the tower facing north (`-Z`). Sim events carry sim-space positions; they are played
//! spatially at the matching world position, which is how the player locates a bell or a groan in
//! the dark (GDD §4, "Information from sound").

use bevy::audio::{DefaultSpatialScale, SpatialScale, Volume};
use bevy::prelude::*;

use crate::sim::Event as SimEvent;

/// rodio attenuates a spatial source by `min(1, 1/d²)` measured on the *scaled* distance between
/// emitter and ears. With this scale everything within ~33 world units plays at full volume (the
/// island and its surroundings) while the sea edge at 100 units drops to ~0.11 linear: clearly
/// distant, still audible.
const SPATIAL_SCALE: f32 = 0.03;
/// Distance between the listener's ears, in world units. Only affects left/right panning.
const EAR_GAP: f32 = 6.0;
/// The lantern room, where the keeper stands.
const LISTENER_HEIGHT: f32 = 14.0;
/// Height above the water at which sim events are spatialized.
const EVENT_HEIGHT: f32 = 0.5;
const AMBIENCE_VOLUME: f32 = 0.25;
/// Volume of the rotation creak while the beam is turning.
const CREAK_VOLUME: f32 = 0.35;
/// Fraction of the remaining creak volume gap closed per call to [`set_rotating`], so the
/// mechanism fades in and out over a few frames instead of clicking.
const CREAK_FADE: f32 = 0.12;

/// Registers the procedural sound bank, the looping beds and the spatial listener.
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_audio);
    }
}

/// Marks the looping mechanism creak, whose volume tracks whether the beam is rotating.
#[derive(Component)]
pub struct MechanismLoop;

/// Every one-shot cue in the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Ignite,
    Bell,
    CreatureCall,
    Splash,
    Rescue,
    Capture,
    LayerTick,
    Transform,
    Dawn,
    VoyageJoin,
    Blocked,
}

impl Cue {
    const COUNT: usize = 11;

    /// Every cue, in discriminant order: indexes [`Sounds::cues`].
    pub const ALL: [Cue; Self::COUNT] = [
        Cue::Ignite,
        Cue::Bell,
        Cue::CreatureCall,
        Cue::Splash,
        Cue::Rescue,
        Cue::Capture,
        Cue::LayerTick,
        Cue::Transform,
        Cue::Dawn,
        Cue::VoyageJoin,
        Cue::Blocked,
    ];
}

/// The synthesized sound bank, available once the startup system has run.
#[derive(Resource)]
pub struct Sounds {
    /// One handle per [`Cue`], indexed by discriminant.
    cues: [Handle<AudioSource>; Cue::COUNT],
    /// Looping sea bed, spawned by the plugin at startup.
    pub ambience: Handle<AudioSource>,
    /// Looping rotation creak, spawned by the plugin at startup (silent until the beam turns).
    pub creak: Handle<AudioSource>,
}

impl Sounds {
    /// The raw asset handle for a cue, for callers that want to drive playback themselves.
    pub fn handle(&self, cue: Cue) -> Handle<AudioSource> {
        self.cues[cue as usize].clone()
    }

    /// Spawn a one-shot. `position: Some(world_pos)` plays spatially (the entity despawns when the
    /// sound ends); `None` plays non-spatial.
    pub fn play(&self, commands: &mut Commands, cue: Cue, position: Option<Vec3>, volume: f32) {
        let settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume.max(0.0)));
        let player = AudioPlayer::new(self.handle(cue));
        match position {
            Some(pos) => {
                commands.spawn((
                    player,
                    settings.with_spatial(true),
                    Transform::from_translation(pos),
                ));
            }
            None => {
                commands.spawn((player, settings));
            }
        }
    }

    /// Render every cue once and register it as an audio asset.
    fn synthesize(sources: &mut Assets<AudioSource>) -> Sounds {
        let mut add = |samples: Vec<f32>| {
            sources.add(AudioSource {
                bytes: wav_bytes(&samples, SAMPLE_RATE).into(),
            })
        };
        Sounds {
            cues: Cue::ALL.map(|cue| add(render(cue))),
            ambience: add(ambience()),
            creak: add(creak()),
        }
    }
}

/// Map a simulation event to an audio cue, spatialized at the event's sim position when it has
/// one. Called once per event, every frame.
pub fn play_event(commands: &mut Commands, sounds: &Sounds, event: &SimEvent) {
    let at = |pos: &glam::Vec2| Some(crate::app::to_world_h(*pos, EVENT_HEIGHT));
    match event {
        SimEvent::Ignite => sounds.play(commands, Cue::Ignite, None, 0.9),
        SimEvent::NightBegins => {}
        SimEvent::Dawn => sounds.play(commands, Cue::Dawn, None, 0.7),
        // A ship announcing itself out of the dark: deliberately quiet, the player has to listen.
        SimEvent::ShipArrived { pos, .. } => sounds.play(commands, Cue::Bell, at(pos), 0.35),
        SimEvent::CreatureAppears { pos, .. } => {
            sounds.play(commands, Cue::CreatureCall, at(pos), 0.8)
        }
        SimEvent::Rescued { pos, .. } => sounds.play(commands, Cue::Rescue, at(pos), 0.8),
        SimEvent::Sunk { pos, .. } => sounds.play(commands, Cue::Splash, at(pos), 0.85),
        SimEvent::Wrecked { pos, .. } => sounds.play(commands, Cue::Splash, at(pos), 0.7),
        SimEvent::Transformed { pos, .. } => sounds.play(commands, Cue::Transform, at(pos), 0.75),
        SimEvent::Bell { pos } => sounds.play(commands, Cue::Bell, at(pos), 0.6),
        SimEvent::CreatureCall { pos } => sounds.play(commands, Cue::CreatureCall, at(pos), 0.7),
        SimEvent::LayerChanged { .. } => sounds.play(commands, Cue::LayerTick, None, 0.3),
        SimEvent::Captured { .. } => sounds.play(commands, Cue::Capture, None, 0.55),
        SimEvent::VoyageBegins => {}
        SimEvent::VoyageDelay { pos } => sounds.play(commands, Cue::Splash, at(pos), 0.3),
        SimEvent::VoyageBlocked { pos } => sounds.play(commands, Cue::Blocked, at(pos), 0.7),
        SimEvent::VoyageJoined { pos, .. } => sounds.play(commands, Cue::VoyageJoin, at(pos), 0.7),
        SimEvent::VoyageArrived => sounds.play(commands, Cue::Rescue, None, 0.8),
        SimEvent::SessionEnded => {}
    }
}

/// Set whether the beam is currently rotating; drives the mechanism creak loop volume. Called
/// every frame, and eases towards the target so starting and stopping does not click.
pub fn set_rotating(sinks: &mut Query<&mut AudioSink, With<MechanismLoop>>, rotating: bool) {
    let target = if rotating { CREAK_VOLUME } else { 0.0 };
    for mut sink in sinks.iter_mut() {
        let current = sink.volume().to_linear();
        let mut next = current + (target - current) * CREAK_FADE;
        if (target - next).abs() < 1.0e-3 {
            next = target;
        }
        if (next - current).abs() > 1.0e-5 {
            sink.set_volume(Volume::Linear(next));
        }
    }
}

fn setup_audio(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let sounds = Sounds::synthesize(&mut sources);

    commands.insert_resource(DefaultSpatialScale(SpatialScale::new(SPATIAL_SCALE)));

    commands.spawn((
        Name::new("Ambience"),
        AudioPlayer::new(sounds.ambience.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(AMBIENCE_VOLUME)),
    ));

    // Starts silent; `set_rotating` fades it in while the beam turns.
    commands.spawn((
        Name::new("Mechanism"),
        MechanismLoop,
        AudioPlayer::new(sounds.creak.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
    ));

    // Identity rotation already looks down `-Z`, which is north in sim space.
    commands.spawn((
        Name::new("Listener"),
        SpatialListener::new(EAR_GAP),
        Transform::from_xyz(0.0, LISTENER_HEIGHT, 0.0),
    ));

    commands.insert_resource(sounds);
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

const SAMPLE_RATE: u32 = 44_100;
const SR: f32 = SAMPLE_RATE as f32;
const TAU: f32 = core::f32::consts::TAU;

/// A valid RIFF/WAVE mono PCM16 file for `samples`, which are limited to `[-1, 1]` on the way out.
fn wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    const HEADER: usize = 44;
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(HEADER + data_len);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((HEADER - 8 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // format: PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // channels: mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &s in samples {
        let q = (limit(s) * 32_767.0) as i16;
        out.extend_from_slice(&q.to_le_bytes());
    }
    out
}

/// Soft knee above 0.7, hard ceiling at 1. Keeps peaks from wrapping without dulling normal levels.
#[inline]
fn limit(x: f32) -> f32 {
    const KNEE: f32 = 0.7;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

/// xorshift32: deterministic noise, so every build sounds identical.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Rng(seed | 1)
    }

    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// White noise in `[-1, 1)`.
    #[inline]
    fn noise(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }
}

/// One-pole low-pass with a per-sample cutoff, for sweeping filters.
#[derive(Default)]
struct OnePole(f32);

impl OnePole {
    #[inline]
    fn process(&mut self, x: f32, cutoff_hz: f32) -> f32 {
        let c = cutoff_hz.clamp(10.0, SR * 0.45);
        let a = 1.0 - (-TAU * c / SR).exp();
        self.0 += a * (x - self.0);
        self.0
    }
}

#[inline]
fn samples(seconds: f32) -> usize {
    (seconds * SR) as usize
}

#[inline]
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Linear attack then exponential decay, peaking at 1.
#[inline]
fn ad(t: f32, attack: f32, decay: f32) -> f32 {
    if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

fn peak_of(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

fn normalize(buf: &mut [f32], peak: f32) {
    let max = peak_of(buf);
    if max > 1.0e-6 {
        let gain = peak / max;
        for s in buf.iter_mut() {
            *s *= gain;
        }
    }
}

/// Fade a one-shot's edges so it cannot click, then normalize, so cues whose peak *is* the
/// opening transient still reach their target level.
fn finish(mut buf: Vec<f32>, peak: f32) -> Vec<f32> {
    let n = buf.len();
    let fade_in = samples(0.002).min(n / 4);
    let fade_out = samples(0.006).min(n / 4);
    for i in 0..fade_in {
        buf[i] *= i as f32 / fade_in as f32;
    }
    for i in 0..fade_out {
        buf[n - 1 - i] *= i as f32 / fade_out as f32;
    }
    normalize(&mut buf, peak);
    buf
}

/// Crossfade `fade` samples of tail into the head so the first `n` samples loop seamlessly.
fn seamless(mut buf: Vec<f32>, n: usize, fade: usize) -> Vec<f32> {
    debug_assert_eq!(buf.len(), n + fade);
    for i in 0..fade {
        let w = i as f32 / fade as f32;
        buf[i] = buf[i] * w + buf[n + i] * (1.0 - w);
    }
    buf.truncate(n);
    buf
}

fn render(cue: Cue) -> Vec<f32> {
    match cue {
        Cue::Ignite => ignite(),
        Cue::Bell => bell(),
        Cue::CreatureCall => creature_call(),
        Cue::Splash => splash(),
        Cue::Rescue => rescue(),
        Cue::Capture => capture(),
        Cue::LayerTick => layer_tick(),
        Cue::Transform => transform(),
        Cue::Dawn => dawn(),
        Cue::VoyageJoin => voyage_join(),
        Cue::Blocked => blocked(),
    }
}

/// The lamp catching: a filtered noise whoosh over a low rumble.
fn ignite() -> Vec<f32> {
    let dur = 1.5;
    let n = samples(dur);
    let mut rng = Rng::new(0x1357_9BDF);
    let (mut lp1, mut lp2) = (OnePole::default(), OnePole::default());
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let u = t / dur;
        let arc = (u * core::f32::consts::PI).sin();
        // The filter opens as the flame takes hold, then closes as it settles.
        let cutoff = 180.0 + 3200.0 * arc * arc;
        let air = lp2.process(lp1.process(rng.noise(), cutoff), cutoff);
        let f = 62.0 - 18.0 * u;
        phase += f / SR;
        let rumble = (TAU * phase).sin() + 0.22 * (TAU * 2.0 * phase).sin();
        out.push(air * 4.0 * arc + rumble * ad(t, 0.12, 0.55) * 0.45);
    }
    finish(out, 0.85)
}

/// A ship's bell: inharmonic partials over a strike transient.
fn bell() -> Vec<f32> {
    let dur = 1.4;
    let n = samples(dur);
    // (frequency, amplitude, decay seconds)
    let partials: [(f32, f32, f32); 4] = [
        (880.0, 1.0, 1.55),
        (1409.0, 0.55, 1.05),
        (2200.0, 0.30, 0.65),
        (3320.0, 0.13, 0.35),
    ];
    let mut rng = Rng::new(0x00BE_11A1);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let mut s = 0.0;
        for (k, &(f, a, decay)) in partials.iter().enumerate() {
            // A slightly detuned twin per partial gives the slow beating of struck metal.
            let detune = 1.0 + 0.0016 * (k as f32 + 1.0);
            let e = (-t / decay).exp();
            s += a * e * ((TAU * f * t).sin() + 0.6 * (TAU * f * detune * t).sin());
        }
        let strike = if t < 0.012 {
            rng.noise() * (1.0 - t / 0.012) * 0.9
        } else {
            0.0
        };
        out.push(s * 0.3 * (t / 0.004).min(1.0) + strike);
    }
    finish(out, 0.8)
}

/// Something large out in the dark: a slow groan sliding down.
fn creature_call() -> Vec<f32> {
    let dur = 1.8;
    let n = samples(dur);
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let u = t / dur;
        let f0 = 70.0 - 25.0 * u;
        phase += f0 / SR;
        let s = (TAU * phase).sin()
            + 0.45 * (TAU * 2.0 * phase).sin()
            + 0.22 * (TAU * 3.0 * phase).sin()
            + 0.09 * (TAU * 5.0 * phase).sin();
        let tremolo = 0.76 + 0.24 * (TAU * 3.1 * t).sin();
        let env = smoothstep((t / 0.35).min(1.0)) * smoothstep(((dur - t) / 0.5).clamp(0.0, 1.0));
        out.push(s * 0.32 * tremolo * env);
    }
    finish(out, 0.85)
}

/// Water closing over something.
fn splash() -> Vec<f32> {
    let dur = 0.7;
    let n = samples(dur);
    let mut rng = Rng::new(0x5B1A_5B05);
    let (mut lp1, mut lp2, mut deep) = (OnePole::default(), OnePole::default(), OnePole::default());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let w = rng.noise();
        let cutoff = 260.0 + 5200.0 * (-t / 0.10).exp();
        let burst = lp2.process(lp1.process(w, cutoff), cutoff) * ad(t, 0.004, 0.12) * 3.0;
        // The heavy body of the water lingers a little longer.
        let body = deep.process(w, 380.0) * (-t / 0.30).exp() * 1.4;
        out.push(burst + body);
    }
    finish(out, 0.85)
}

/// A life saved: two rising chime notes.
fn rescue() -> Vec<f32> {
    let dur = 0.9;
    let n = samples(dur);
    let mut out = Vec::with_capacity(n);
    let voice = |f: f32, t: f32| {
        0.6 * (TAU * f * t).sin() + 0.22 * (TAU * 2.0 * f * t).sin() + 0.08 * (TAU * 3.0 * f * t).sin()
    };
    for i in 0..n {
        let t = i as f32 / SR;
        let mut s = voice(660.0, t) * ad(t, 0.008, 0.26);
        let t2 = t - 0.28;
        if t2 > 0.0 {
            s += voice(990.0, t2) * ad(t2, 0.008, 0.34);
        }
        out.push(s);
    }
    finish(out, 0.8)
}

/// A sector locked in: a short bright blip.
fn capture() -> Vec<f32> {
    let dur = 0.15;
    let n = samples(dur);
    let mut rng = Rng::new(0x0CA9_7071);
    let mut lp = OnePole::default();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let tone = ((TAU * 1200.0 * t).sin() + 0.3 * (TAU * 2400.0 * t).sin()) * (-t / 0.028).exp();
        let tick = lp.process(rng.noise(), 5000.0) * (-t / 0.004).exp() * 0.5;
        out.push(tone * 0.8 + tick);
    }
    finish(out, 0.7)
}

/// Guidance layer changed: an unobtrusive two-tone tick.
fn layer_tick() -> Vec<f32> {
    let dur = 0.3;
    let n = samples(dur);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let mut s = (TAU * 1480.0 * t).sin() * (-t / 0.012).exp();
        let t2 = t - 0.1;
        if t2 > 0.0 {
            s += (TAU * 990.0 * t2).sin() * (-t2 / 0.022).exp() * 0.85;
        }
        out.push(s);
    }
    finish(out, 0.45)
}

/// The Mutable Sea reshaping a hull: detuned sines gliding down an octave.
fn transform() -> Vec<f32> {
    let dur = 1.2;
    let n = samples(dur);
    let ratios = [1.0f32, 1.004, 0.995, 1.5];
    let amps = [1.0f32, 0.7, 0.7, 0.26];
    let mut phases = [0.0f32; 4];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let u = t / dur;
        let glide = 660.0 * 2.0f32.powf(-u);
        let mut s = 0.0;
        for k in 0..4 {
            phases[k] += glide * ratios[k] / SR;
            s += amps[k] * (TAU * phases[k]).sin();
        }
        // Shimmer that slows as the change takes hold.
        let shimmer = 0.72 + 0.28 * (TAU * (11.0 - 6.0 * u) * t).sin();
        let env = smoothstep((t / 0.09).min(1.0)) * smoothstep(((dur - t) / 0.4).clamp(0.0, 1.0));
        out.push(s * 0.22 * shimmer * env);
    }
    finish(out, 0.8)
}

/// The night survived: a warm pad swelling with the light.
fn dawn() -> Vec<f32> {
    let dur = 3.0;
    let n = samples(dur);
    let partials: [(f32, f32); 5] = [
        (110.0, 1.0),
        (165.0, 0.68),
        (220.0, 0.52),
        (330.0, 0.30),
        (440.0, 0.16),
    ];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let mut s = 0.0;
        for (k, &(f, a)) in partials.iter().enumerate() {
            let detune = 1.0 + 0.0012 * (k as f32 + 1.0);
            s += a * ((TAU * f * t).sin() + 0.5 * (TAU * f * detune * t + 1.1).sin());
        }
        let env = smoothstep(t / 1.5) * smoothstep((dur - t) / 0.9);
        let breath = 0.92 + 0.08 * (TAU * 0.6 * t).sin();
        out.push(s * 0.2 * env * breath);
    }
    finish(out, 0.75)
}

/// A ship falling in behind the convoy: a short rising triad.
fn voyage_join() -> Vec<f32> {
    let dur = 0.6;
    let n = samples(dur);
    let notes: [(f32, f32); 3] = [(523.25, 0.0), (659.25, 0.11), (783.99, 0.22)];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let mut s = 0.0;
        for &(f, start) in &notes {
            let tn = t - start;
            if tn > 0.0 {
                s += ((TAU * f * tn).sin() + 0.25 * (TAU * 2.0 * f * tn).sin())
                    * ad(tn, 0.006, 0.20);
            }
        }
        out.push(s * 0.5);
    }
    finish(out, 0.75)
}

/// A voyage refused: a wooden thud and a creak of protest.
fn blocked() -> Vec<f32> {
    let dur = 0.8;
    let n = samples(dur);
    let mut rng = Rng::new(0x0B10_C4ED);
    let (mut knock_lp, mut rasp_lp) = (OnePole::default(), OnePole::default());
    let mut phase = 0.0f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let f = 74.0 + 91.0 * (-t / 0.05).exp();
        phase += f / SR;
        let thud = (TAU * phase).sin() * (-t / 0.085).exp();
        let knock = knock_lp.process(rng.noise(), 1400.0) * (-t / 0.012).exp() * 0.7;
        let rasp = rasp_lp.process(rng.noise(), 900.0);
        let ct = t - 0.1;
        let creak = if ct > 0.0 {
            rasp * (0.45 + 0.55 * (TAU * 27.0 * ct).sin())
                * smoothstep(ct / 0.06)
                * (-ct / 0.22).exp()
                * 1.6
        } else {
            0.0
        };
        out.push(thud * 0.9 + knock + creak);
    }
    finish(out, 0.8)
}

/// Four seconds of sea, looped: low-passed noise with a swell that is periodic over the loop.
fn ambience() -> Vec<f32> {
    let dur = 4.0;
    let n = samples(dur);
    let fade = samples(0.4);
    let mut rng = Rng::new(0x5EA5_1DE5);
    let (mut deep1, mut deep2, mut wash) =
        (OnePole::default(), OnePole::default(), OnePole::default());
    let mut buf = Vec::with_capacity(n + fade);
    for _ in 0..n + fade {
        let w = rng.noise();
        let low = deep2.process(deep1.process(w, 210.0), 210.0);
        let hiss = wash.process(w, 1700.0);
        buf.push(low * 7.0 + hiss * 0.35);
    }
    let mut buf = seamless(buf, n, fade);
    // Whole numbers of cycles per loop keep the swell continuous across the seam.
    for (i, s) in buf.iter_mut().enumerate() {
        let u = i as f32 / n as f32;
        let swell = 0.72 + 0.20 * (TAU * u).sin() + 0.08 * (TAU * 3.0 * u + 1.7).sin();
        *s *= swell;
    }
    normalize(&mut buf, 0.7);
    buf
}

/// One second of turning mechanism: a low rasp with a faint hum, looped.
fn creak() -> Vec<f32> {
    let dur = 1.0;
    let n = samples(dur);
    let fade = samples(0.12);
    let mut rng = Rng::new(0x0C3E_A11C);
    let (mut lp1, mut lp2) = (OnePole::default(), OnePole::default());
    let mut buf = Vec::with_capacity(n + fade);
    for _ in 0..n + fade {
        buf.push(lp2.process(lp1.process(rng.noise(), 700.0), 300.0) * 6.0);
    }
    let mut buf = seamless(buf, n, fade);
    // 14 Hz rasp and 90 Hz hum: both whole cycle counts over a one-second loop.
    for (i, s) in buf.iter_mut().enumerate() {
        let t = i as f32 / SR;
        let rasp = 0.45 + 0.55 * (TAU * 14.0 * t).sin().abs();
        let hum = 0.16 * (TAU * 90.0 * t).sin() * (0.7 + 0.3 * (TAU * 7.0 * t).sin());
        *s = *s * rasp + hum;
    }
    normalize(&mut buf, 0.6);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn wav_header_describes_the_payload() {
        let samples = vec![0.0, 0.5, -0.5, 1.0];
        let wav = wav_bytes(&samples, SAMPLE_RATE);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(read_u32(&wav, 40) as usize, samples.len() * 2);
        assert_eq!(read_u32(&wav, 4) as usize, wav.len() - 8);
        assert_eq!(read_u32(&wav, 24), SAMPLE_RATE);
        assert_eq!(wav.len(), 44 + samples.len() * 2);
    }

    /// Bevy hands these bytes to `rodio`, whose decoder builder unwraps: a malformed header would
    /// panic the game the first time a cue played.
    #[test]
    fn synthesized_bytes_decode_through_the_engine() {
        use bevy::audio::{Decodable, Source};

        let rendered = bell();
        let source = AudioSource {
            bytes: wav_bytes(&rendered, SAMPLE_RATE).into(),
        };
        let decoder = source.decoder();

        assert_eq!(decoder.sample_rate().get(), SAMPLE_RATE);
        assert_eq!(decoder.channels().get(), 1);
        assert_eq!(decoder.count(), rendered.len());
    }

    #[test]
    fn loud_samples_are_limited_not_wrapped() {
        let wav = wav_bytes(&[4.0, -4.0], SAMPLE_RATE);
        let a = i16::from_le_bytes([wav[44], wav[45]]);
        let b = i16::from_le_bytes([wav[46], wav[47]]);
        assert!(a > 30_000, "positive peak wrapped: {a}");
        assert!(b < -30_000, "negative peak wrapped: {b}");
    }

    #[test]
    fn every_cue_renders_audible_unclipped_audio() {
        for cue in Cue::ALL {
            let buf = render(cue);
            assert!(!buf.is_empty(), "{cue:?} is empty");
            let peak = peak_of(&buf);
            assert!(peak > 0.3, "{cue:?} is too quiet: {peak}");
            assert!(peak <= 1.0, "{cue:?} clips: {peak}");
            assert!(buf.iter().all(|s| s.is_finite()), "{cue:?} has non-finite samples");
        }
    }

    #[test]
    fn loops_are_seamless_at_the_seam() {
        for (name, buf) in [("ambience", ambience()), ("creak", creak())] {
            let n = buf.len();
            // The step across the loop point must be no worse than a typical step inside it.
            let inside = (1..n).map(|i| (buf[i] - buf[i - 1]).abs()).fold(0.0f32, f32::max);
            let seam = (buf[0] - buf[n - 1]).abs();
            assert!(
                seam <= inside,
                "{name} clicks at the loop point: seam {seam} vs max inner step {inside}"
            );
        }
    }
}
