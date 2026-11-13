//! Kontinuum's engine: six layers of ambient sound (drone, pad, texture, bells, percussion and
//! choir), the director that composes with them by itself, the mix and the master effects.
//!
//! [`Engine::render`] fills interleaved stereo f32 at [`RATE`]; the work is done in blocks of
//! [`BLOCK`] frames, the director's and the automations' control rate. The engine allocates
//! nothing once made, so it can run in an audio callback; [`Command`]s change it between
//! blocks. Given the same seed, settings and commands at the same frames, it renders the same
//! samples: an export is the music the app plays.

pub mod director;
pub mod layers;
pub mod music;
pub mod recipe;

use knt_dsp::{Chorus, Delay, OnePole, Reverb, Smooth, soft_clip};
use serde::{Deserialize, Serialize};

pub use director::{AUTOS, Director, Event};
use layers::{Bells, Block, Choir, Drone, Modular, Pad, Perc, Texture, block_level};
pub use layers::{DronePatch, Hit, ModularPatch, PadPatch};
pub use music::{Chord, Scale, note_name};
pub use recipe::{CARTA, Levels, Recipe};

/// The output rate.
pub const RATE: u32 = 44_100;
/// The frames of a control block.
pub const BLOCK: usize = 64;

/// The layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Drone,
    Pad,
    Texture,
    Bells,
    Perc,
    Choir,
    Modular,
}

/// The number of layers.
pub const LAYERS: usize = 7;

impl Layer {
    pub const ALL: [Layer; LAYERS] = [
        Layer::Drone,
        Layer::Pad,
        Layer::Texture,
        Layer::Bells,
        Layer::Perc,
        Layer::Choir,
        Layer::Modular,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Layer::Drone => "Drone",
            Layer::Pad => "Pad",
            Layer::Texture => "Texture",
            Layer::Bells => "Bells",
            Layer::Perc => "Percussion",
            Layer::Choir => "Choir",
            Layer::Modular => "Modular",
        }
    }

    /// The layer's sends to the reverb and the delay.
    fn sends(self) -> (f32, f32) {
        match self {
            Layer::Drone => (0.35, 0.0),
            Layer::Pad => (0.5, 0.15),
            Layer::Texture => (0.4, 0.05),
            Layer::Bells => (0.7, 0.3),
            Layer::Perc => (0.6, 0.25),
            Layer::Choir => (0.85, 0.2),
            Layer::Modular => (0.5, 0.45),
        }
    }
}

/// Who makes the music when someone plays a MIDI keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The director plays along: it takes the chords played as its harmony and leaves room.
    #[default]
    Accompany,
    /// Only what is played sounds.
    Solo,
    /// The director rests while someone plays, and takes over after [`Params::relay_seconds`]
    /// without a note.
    Relay,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Accompany, Mode::Solo, Mode::Relay];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Accompany => "Accompany",
            Mode::Solo => "Solo",
            Mode::Relay => "Relay",
        }
    }
}

/// The settings the user changes (knobs, faders, switches).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    /// The master volume, 0..1.
    pub volume: f32,
    /// Each layer's fader, 0..1 (0.8: as the recipe has it).
    pub levels: Levels,
    pub mutes: [bool; LAYERS],
    /// How much the director's automations move things, 0..1.
    pub evolution: f32,
    /// Biases over the recipe, -1..1.
    pub pace_bias: f32,
    pub brightness_bias: f32,
    pub density_bias: f32,
    pub space_bias: f32,
    pub echo_bias: f32,
    /// The tape warmth, 0..1.
    pub warmth: f32,
    pub mode: Mode,
    /// Relay mode: the seconds without a note after which the director takes over.
    pub relay_seconds: f32,
    /// The layer the keyboard plays.
    pub play_layer: Layer,
    /// The director keeps the chord.
    pub hold_harmony: bool,
    /// The sleep timer: fade to silence after this many minutes (0: off).
    pub sleep_minutes: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            volume: 0.8,
            levels: Levels {
                drone: 0.8,
                pad: 0.8,
                texture: 0.8,
                bells: 0.8,
                perc: 0.8,
                choir: 0.8,
                modular: 0.8,
            },
            mutes: [false; LAYERS],
            evolution: 0.6,
            pace_bias: 0.0,
            brightness_bias: 0.0,
            density_bias: 0.0,
            space_bias: 0.0,
            echo_bias: 0.0,
            warmth: 0.2,
            mode: Mode::Accompany,
            relay_seconds: 120.0,
            play_layer: Layer::Pad,
            hold_harmony: false,
            sleep_minutes: 0.0,
        }
    }
}

/// What the app sends to the engine.
#[derive(Debug, Clone)]
pub enum Command {
    Params(Box<Params>),
    Recipe(Box<Recipe>),
    /// Start again from a new seed (the music fades over).
    Seed(u64),
    /// Play (fade in) or stop (fade out).
    Play(bool),
    NextSection,
    /// A key of the MIDI keyboard.
    NoteOn {
        note: u8,
        vel: u8,
    },
    NoteOff {
        note: u8,
    },
    /// A drum pad (General MIDI drum notes).
    Drum {
        note: u8,
        vel: u8,
    },
}

/// The most recent bells the stage shows.
pub const RECENT: usize = 8;

/// What the app and the stage read back after each render.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Snapshot {
    /// Seconds of music since the start.
    pub time: f64,
    pub playing: bool,
    /// The master fade, 0..1.
    pub fade: f32,
    pub seed: u64,
    /// The recipe playing (a drift's current dish) and the one chosen.
    pub recipe: u16,
    pub chosen: u16,
    pub root: i32,
    pub scale: Option<Scale>,
    pub chord: Chord,
    /// How far the chord has gone, 0..1.
    pub chord_progress: f32,
    pub section: u32,
    pub pulse: bool,
    pub choir: bool,
    /// Each layer's level (a meter) and gain.
    pub levels: [f32; LAYERS],
    pub gains: [f32; LAYERS],
    /// The timbre each layer plays now (its name).
    pub patches: [&'static str; LAYERS],
    /// The layers the director has in the current section.
    pub active: [bool; LAYERS],
    pub master: [f32; 2],
    pub drone_cutoff: f32,
    pub texture_center: f32,
    pub vowel: f32,
    /// The last bells: (note, seconds ago).
    pub bells: [(i32, f32); RECENT],
    pub bells_played: u32,
    pub perc_played: u32,
    pub last_hit: Hit,
    pub drops_played: u32,
    pub auto: [f32; 6],
    /// The latest events, newest first: (event, seconds ago).
    pub events: [Option<(Event, f32)>; 8],
    pub user_active: bool,
    pub leads: bool,
    pub mode: Mode,
    /// Seconds until the sleep timer ends the music (None: off).
    pub sleep_left: Option<f32>,
}

/// The engine.
pub struct Engine {
    pub params: Params,
    seed: u64,
    director: Director,
    drone: Drone,
    pad: Pad,
    texture: Texture,
    bells: Bells,
    perc: Perc,
    choir: Choir,
    modular: Modular,
    bufs: [Block; LAYERS],
    gains: [Smooth; LAYERS],
    meters: [OnePole; LAYERS],
    /// A high-pass under the music (the rumble a small speaker cannot play only muddies it).
    hp: [OnePole; 2],
    master_meter: [OnePole; 2],
    chorus: Chorus,
    delay: Delay,
    reverb: Reverb,
    reverb_set: (f32, f32),
    fade: Smooth,
    playing: bool,
    limiter: f32,
    /// The leveler: the music's slow loudness and the gain that brings it to a steady level,
    /// so a sparse section is not much quieter than a full one (and a stream stays even).
    loudness: f32,
    level_gain: f32,
    /// The current block's output and how much of it is handed out.
    out: Block,
    pos: usize,
    frame: u64,
    sleep_elapsed: f32,
}

impl Engine {
    /// An engine for `recipe` from `seed`, stopped (it fades in on [`Command::Play`]).
    pub fn new(seed: u64, recipe: Recipe, params: Params) -> Self {
        let mut rng = knt_dsp::Rng::new(seed ^ 0x004B_4E54);
        let gains = Layer::ALL.map(|_| Smooth::new(0.0, 5.0));
        Engine {
            params,
            seed,
            director: Director::new(seed, recipe),
            drone: Drone::new(&mut rng),
            pad: Pad::new(&mut rng),
            texture: Texture::new(&mut rng),
            bells: Bells::new(),
            perc: Perc::new(&mut rng),
            choir: Choir::new(&mut rng),
            modular: Modular::new(&mut rng),
            bufs: [[[0.0; 2]; BLOCK]; LAYERS],
            gains,
            meters: [OnePole::default(); LAYERS],
            hp: [OnePole::default(); 2],
            master_meter: [OnePole::default(); 2],
            chorus: Chorus::new(),
            delay: Delay::new(),
            reverb: Reverb::new(),
            reverb_set: (-1.0, -1.0),
            fade: Smooth::new(0.0, 2.0),
            playing: false,
            limiter: 1.0,
            loudness: 0.1,
            level_gain: 1.0,
            out: [[0.0; 2]; BLOCK],
            pos: BLOCK,
            frame: 0,
            sleep_elapsed: 0.0,
        }
    }

    pub fn director(&self) -> &Director {
        &self.director
    }

    /// Applies a command; returns the box it replaced, for the caller to drop (an engine in an
    /// audio callback frees nothing).
    pub fn command(&mut self, c: Command) -> Option<Command> {
        match c {
            Command::Params(mut p) => {
                if p.sleep_minutes != self.params.sleep_minutes {
                    self.sleep_elapsed = 0.0;
                }
                std::mem::swap(&mut self.params, &mut p);
                return Some(Command::Params(p));
            }
            Command::Recipe(r) => {
                self.director.set_recipe(*r);
                return Some(Command::Recipe(r));
            }
            Command::Seed(s) => {
                // the new director starts where the old one was: same recipe, new choices
                let recipe = self.director.chosen;
                self.seed = s;
                self.director = Director::new(s, recipe);
            }
            Command::Play(on) => {
                self.playing = on;
                if on {
                    self.sleep_elapsed = 0.0;
                }
            }
            Command::NextSection => self.director.next_section(),
            Command::NoteOn { note, vel } => {
                let v = vel as f32 / 127.0;
                self.director.note_on(note);
                let n = note as i32;
                match self.params.play_layer {
                    Layer::Pad => self.pad.note_on(n, v),
                    Layer::Bells => {
                        let mut rng = knt_dsp::Rng::new(self.frame ^ n as u64);
                        self.bells.strike(n, v, &mut rng, self.frame);
                    }
                    Layer::Drone => self.drone.set_notes(&[n]),
                    Layer::Choir => self.update_choir_from_keys(),
                    Layer::Perc => {
                        if let Some(h) = Hit::from_note(note) {
                            self.perc.hit(h, v, n);
                        }
                    }
                    Layer::Texture | Layer::Modular => self.pad.note_on(n, v),
                }
            }
            Command::NoteOff { note } => {
                self.director.note_off(note);
                self.pad.note_off(note as i32);
                if self.params.play_layer == Layer::Choir {
                    self.update_choir_from_keys();
                }
            }
            Command::Drum { note, vel } => {
                if let Some(h) = Hit::from_note(note) {
                    self.director.note_on(0);
                    self.director.note_off(0);
                    let pitch = self.director.root + 12;
                    self.perc.hit(h, vel as f32 / 127.0, pitch);
                }
            }
        }
        None
    }

    fn update_choir_from_keys(&mut self) {
        let mut v = [0i32; 16];
        let n = self.director.held(&mut v);
        self.choir.set_notes(&v[..n.min(6)]);
    }

    /// Fills `out` (interleaved stereo) with the music.
    pub fn render(&mut self, out: &mut [f32]) {
        for f in out.as_chunks_mut::<2>().0.iter_mut() {
            if self.pos == BLOCK {
                self.block();
                self.pos = 0;
            }
            let s = self.out[self.pos];
            f[0] = s[0];
            f[1] = s[1];
            self.pos += 1;
        }
    }

    /// One block.
    fn block(&mut self) {
        let p = self.params;
        let dt = BLOCK as f32 / RATE as f32;
        let d = &mut self.director;
        // the director composes only while the music plays
        if self.playing || self.fade.value > 0.001 {
            d.step(
                &p,
                &mut self.drone,
                &mut self.pad,
                &mut self.choir,
                &mut self.bells,
                &mut self.perc,
                &mut self.modular,
                self.frame,
            );
        }
        let r = d.recipe;
        let auto = d.auto;
        let bright = (r.brightness + p.brightness_bias * 0.5 + auto[0] * 0.3).clamp(0.0, 1.0);
        let motion = (0.4 + auto[1] * 0.5).clamp(0.0, 1.0);
        let color = (bright * 0.6 + auto[2] * 0.4).clamp(0.0, 1.0);
        let rain = (r.density * 0.6 + auto[3] * 0.4 + p.density_bias * 0.3).clamp(0.0, 1.0)
            * if r.levels.texture > 0.5 { 1.0 } else { 0.5 };
        // the layers
        self.drone.render(&mut self.bufs[0], bright * 0.8, motion);
        self.pad.render(&mut self.bufs[1], bright, motion);
        self.texture.render(&mut self.bufs[2], color, motion, rain);
        self.bells.render(&mut self.bufs[3]);
        self.perc.render(&mut self.bufs[4]);
        self.choir.render(&mut self.bufs[5]);
        let leads_now = d.leads(&p);
        self.modular.render(
            &mut self.bufs[6],
            leads_now && d.active[Layer::Modular.index()],
            r.bpm,
            bright,
            motion,
        );
        // the gains: the user's fader (0.8 = as the recipe) times the recipe's level, the
        // percussion and the choir only in their sections (or when played)
        let leads = d.leads(&p);
        for (i, l) in Layer::ALL.into_iter().enumerate() {
            let mut g = p.levels.get(l) / 0.8 * r.levels.get(l);
            if p.mutes[i] {
                g = 0.0;
            }
            // the section's orchestration (the keyboard's layer always sounds when played)
            if !d.active[i] && !(d.user_active && p.play_layer == l) {
                g = 0.0;
            }
            match l {
                Layer::Perc => {
                    if !(d.pulse && leads) && !(d.user_active && p.play_layer == Layer::Perc) {
                        g *= 0.0;
                    }
                }
                Layer::Choir if !(d.choir || d.user_active && p.play_layer == Layer::Choir) => {
                    g *= 0.0;
                }
                _ => {}
            }
            // the user's notes play even when the director rests
            self.gains[i].target = g;
        }
        // perc hits played by hand ring even out of a pulse section
        if d.user_active && p.play_layer == Layer::Perc {
            self.gains[4].target = self.gains[4].target.max(p.levels.perc / 0.8 * 0.5);
        }
        // the space
        let space = (r.space + p.space_bias * 0.5 + auto[4] * 0.2).clamp(0.0, 1.0);
        let decay = 3.0 + 17.0 * space;
        if (space - self.reverb_set.0).abs() > 0.01 || (decay - self.reverb_set.1).abs() > 0.1 {
            self.reverb.set(space, decay, 0.45);
            self.reverb_set = (space, decay);
        }
        let echo = (r.echo + p.echo_bias * 0.5).clamp(0.0, 1.0);
        let delay_s = 60.0 / r.bpm.clamp(40.0, 140.0) * 0.75 * 2.0;
        let fb = 0.25 + 0.45 * echo;
        // the sleep timer and the fade
        if p.sleep_minutes > 0.0 && self.playing {
            self.sleep_elapsed += dt;
            if self.sleep_elapsed >= p.sleep_minutes * 60.0 {
                self.playing = false;
            }
        }
        self.fade.target = if self.playing { 1.0 } else { 0.0 };
        let fade = self.fade.step(BLOCK);
        let warmth = 1.0 + 2.5 * p.warmth;
        let mut gains = [0.0f32; LAYERS];
        let hp_c = knt_dsp::coef(1.0 / (std::f32::consts::TAU * 28.0));
        for (g, s) in gains.iter_mut().zip(&mut self.gains) {
            *g = s.step(BLOCK);
        }
        for k in 0..BLOCK {
            let mut dry = [0.0f32; 2];
            let mut rv = [0.0f32; 2];
            let mut dl = [0.0f32; 2];
            for (i, l) in Layer::ALL.into_iter().enumerate() {
                let (rs, ds) = l.sends();
                let x = self.bufs[i][k];
                let g = gains[i];
                for c in 0..2 {
                    let y = x[c] * g;
                    dry[c] += y;
                    rv[c] += y * rs;
                    dl[c] += y * ds;
                }
            }
            let (cl, cr) = self.chorus.process(dry[0], dry[1], 0.35);
            let (el, er) = self.delay.process(dl[0], dl[1], delay_s, fb, 0.4);
            let (wl, wr) = self.reverb.process(rv[0] + el * 0.3, rv[1] + er * 0.3);
            let e = echo * 0.8;
            let mut l = cl + el * e + wl * 0.9;
            let mut r = cr + er * e + wr * 0.9;
            l -= self.hp[0].lp(l, hp_c);
            r -= self.hp[1].lp(r, hp_c);
            // the leveler: about -18 dBFS of RMS, within -6..+8 dB, over ~10 s
            self.loudness += ((l * l + r * r) * 0.5 - self.loudness) * 0.000_002_3;
            let target = (0.125 / self.loudness.max(1e-6).sqrt()).clamp(0.5, 2.5);
            self.level_gain += (target - self.level_gain) * 0.000_005;
            l *= self.level_gain;
            r *= self.level_gain;
            // the tape: a soft saturation, then the volume and the fade
            l = soft_clip(l * warmth) / warmth.sqrt();
            r = soft_clip(r * warmth) / warmth.sqrt();
            let v = p.volume * p.volume * fade;
            l *= v;
            r *= v;
            // a slow limiter under 0 dBFS
            let peak = l.abs().max(r.abs());
            if peak * self.limiter > 0.89 {
                self.limiter = 0.89 / peak;
            } else {
                self.limiter += (1.0 - self.limiter) * 0.00005;
            }
            self.out[k] = [l * self.limiter, r * self.limiter];
        }
        for (i, b) in self.bufs.iter().enumerate() {
            block_level(b, &mut self.meters[i]);
        }
        for c in 0..2 {
            let e = self.out.iter().map(|f| f[c] * f[c]).sum::<f32>() / BLOCK as f32;
            self.master_meter[c].lp(e.sqrt(), 0.1);
        }
        self.frame += BLOCK as u64;
    }

    /// The state for the app and the stage.
    pub fn snapshot(&self) -> Snapshot {
        let d = &self.director;
        let now = self.frame as f64 / RATE as f64;
        let mut events = [None; 8];
        for (slot, (e, t)) in events.iter_mut().zip(d.latest()) {
            *slot = Some((e, (d.time - t) as f32));
        }
        let mut bells = [(0, f32::INFINITY); RECENT];
        for (b, &(n, f)) in bells.iter_mut().zip(&self.bells.recent) {
            if self.bells.played > 0 && f <= self.frame && n != 0 {
                *b = (n, now as f32 - f as f32 / RATE as f32);
            }
        }
        Snapshot {
            time: d.time,
            playing: self.playing,
            fade: self.fade.value,
            seed: self.seed,
            recipe: d.recipe.id,
            chosen: d.chosen.id,
            root: d.root,
            scale: Some(d.scale),
            chord: d.chord,
            chord_progress: if d.chord_len > 0.0 {
                (1.0 - d.chord_left / d.chord_len).clamp(0.0, 1.0)
            } else {
                0.0
            },
            section: d.section,
            pulse: d.pulse,
            choir: d.choir,
            levels: self.meters.map(|m| m.y),
            gains: self.gains.map(|g| g.value),
            patches: [
                self.drone.patch.name(),
                self.pad.patch.name(),
                "air and drops",
                "FM bells",
                "hand percussion",
                "formant choir",
                self.modular.patch.name(),
            ],
            active: d.active,
            master: self.master_meter.map(|m| m.y),
            drone_cutoff: self.drone.cutoff,
            texture_center: self.texture.center,
            vowel: self.choir.vowel,
            bells,
            bells_played: self.bells.played,
            perc_played: self.perc.played,
            last_hit: self.perc.last,
            drops_played: self.texture.drops_played,
            auto: d.auto,
            events,
            user_active: d.user_active,
            leads: d.leads(&self.params),
            mode: self.params.mode,
            sleep_left: (self.params.sleep_minutes > 0.0 && self.playing)
                .then_some(self.params.sleep_minutes * 60.0 - self.sleep_elapsed),
        }
    }
}

/// Renders `seconds` of music from `seed` and `recipe` offline: interleaved stereo f32. The
/// last `fade_out` seconds fade to silence.
pub fn render(seed: u64, recipe: Recipe, params: Params, seconds: f32, fade_out: f32) -> Vec<f32> {
    let mut e = Engine::new(seed, recipe, params);
    e.command(Command::Play(true));
    let frames = (seconds * RATE as f32) as usize;
    let mut out = vec![0.0f32; 2 * frames];
    let fade_from = frames.saturating_sub((fade_out * RATE as f32) as usize);
    for chunk in out.chunks_mut(2 * 4096) {
        e.render(chunk);
    }
    if fade_out > 0.0 {
        let n = frames - fade_from;
        for (k, f) in out[2 * fade_from..].chunks_mut(2).enumerate() {
            let g = 1.0 - k as f32 / n.max(1) as f32;
            let g = g * g;
            f[0] *= g;
            f[1] *= g;
        }
    }
    out
}

#[cfg(test)]
mod tests;
