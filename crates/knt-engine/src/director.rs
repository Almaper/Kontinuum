//! The director: the composer that plays by itself. It works in sections of a few chords; at
//! each section it may change key or scale, bring in the pulse (the occasional percussion) or
//! the choir, and, in a drifting recipe, move to another dish of the Menu. Within a section it
//! leads chords, strikes bells and moves its automations: slow random walks that push the
//! brightness, the motion, the texture and the space around the user's settings.
//!
//! Everything comes from its random generator, seeded: the same seed and the same settings give
//! the same music.

use knt_dsp::Rng;

use crate::layers::{
    Bells, Choir, Drone, DronePatch, Hit, Modular, ModularPatch, Pad, PadPatch, Perc,
};
use crate::music::{Chord, Scale, chord_from_played};
use crate::recipe::{CARTA, Recipe};
use crate::{BLOCK, LAYERS, Layer, Mode, Params};

/// What the director did, for the stage's captions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A new chord: its root (pitch class) and whether it is minor.
    Chord {
        root: i32,
        minor: bool,
    },
    /// A new key.
    Key {
        root: i32,
        scale: Scale,
    },
    Section(u32),
    PulseIn,
    PulseOut,
    ChoirIn,
    ChoirOut,
    /// Another recipe of the Menu (drift).
    Recipe(u16),
    /// The user started playing.
    UserIn,
    /// The user stopped playing (and, in relay mode, the director takes over again).
    UserOut,
    /// A layer comes in with a timbre.
    Enters(Layer, &'static str),
    /// A layer falls silent.
    Leaves(Layer),
}

/// The automations the director moves.
pub const AUTOS: [&str; 6] = [
    "brightness",
    "motion",
    "texture",
    "rain",
    "space",
    "density",
];

/// A slow random walk around 0 (an Ornstein-Uhlenbeck process), between -1 and 1.
#[derive(Debug, Clone, Copy, Default)]
struct Walk {
    value: f32,
    vel: f32,
}

impl Walk {
    fn step(&mut self, rng: &mut Rng, dt: f32, speed: f32) {
        // a gentle second-order walk: smooth curves, no jitter
        self.vel += (rng.gauss() * 0.6 - self.value * 0.35 - self.vel * 0.8) * dt * speed;
        self.value = (self.value + self.vel * dt * speed).clamp(-1.0, 1.0);
    }
}

pub struct Director {
    rng: Rng,
    pub recipe: Recipe,
    /// The recipe the user chose (a drift wanders from it).
    pub chosen: Recipe,
    pub root: i32,
    pub scale: Scale,
    pub chord: Chord,
    pub section: u32,
    chords_left: u32,
    /// Seconds until the next chord, and the chord's length.
    pub chord_left: f32,
    pub chord_len: f32,
    pub pulse: bool,
    pub choir: bool,
    /// The layers that play in this section (the orchestration).
    pub active: [bool; LAYERS],
    walks: [Walk; 6],
    /// The automations' values, -1..1.
    pub auto: [f32; 6],
    bell_wait: f32,
    /// The pulse: its pattern, step and the time to the next step.
    pattern: [[f32; 16]; 5],
    step: usize,
    step_wait: f32,
    bar: u32,
    pub events: [(Option<Event>, f64); 8],
    next_event: usize,
    /// Seconds since the music started.
    pub time: f64,
    /// The notes held on the keyboard.
    held: [bool; 128],
    /// Seconds since the user last played (∞: never).
    pub idle: f32,
    pub user_active: bool,
    /// The vowel the choir moves to.
    vowel_wait: f32,
}

impl Director {
    pub fn new(seed: u64, recipe: Recipe) -> Self {
        let mut d = Director {
            rng: Rng::new(seed),
            recipe,
            chosen: recipe,
            root: recipe.root,
            scale: recipe.scales[0],
            chord: Chord::default(),
            section: 0,
            chords_left: 0,
            chord_left: 0.0,
            chord_len: 1.0,
            pulse: false,
            choir: false,
            active: [false; LAYERS],
            walks: [Walk::default(); 6],
            auto: [0.0; 6],
            bell_wait: 3.0,
            pattern: [[0.0; 16]; 5],
            step: 0,
            step_wait: 0.0,
            bar: 0,
            events: [(None, 0.0); 8],
            next_event: 0,
            time: 0.0,
            held: [false; 128],
            idle: f32::INFINITY,
            user_active: false,
            vowel_wait: 10.0,
        };
        d.scale = d.recipe.scales[d.rng.below(3)];
        d
    }

    /// The recipe to play from now (the next section takes it entirely).
    pub fn set_recipe(&mut self, r: Recipe) {
        self.chosen = r;
        self.recipe = r;
        self.root = r.root;
        self.scale = r.scales[self.rng.below(3)];
        self.chords_left = 0;
        self.chord_left = 0.0;
        self.event(Event::Recipe(r.id));
    }

    /// Ends the section now.
    pub fn next_section(&mut self) {
        self.chords_left = 0;
        self.chord_left = 0.0;
    }

    fn event(&mut self, e: Event) {
        self.events[self.next_event] = (Some(e), self.time);
        self.next_event = (self.next_event + 1) % self.events.len();
    }

    /// The latest events, newest first.
    pub fn latest(&self) -> impl Iterator<Item = (Event, f64)> + '_ {
        let n = self.events.len();
        (1..=n).filter_map(move |k| {
            let (e, t) = self.events[(self.next_event + n - k) % n];
            e.map(|e| (e, t))
        })
    }

    /// The director in charge (not silenced by the mode and the user's playing).
    pub fn leads(&self, p: &Params) -> bool {
        match p.mode {
            Mode::Accompany => true,
            Mode::Solo => false,
            Mode::Relay => !self.user_active,
        }
    }

    // ---- the keyboard ----

    pub fn note_on(&mut self, n: u8) {
        self.held[n as usize & 127] = true;
        self.idle = 0.0;
        if !self.user_active {
            self.user_active = true;
            self.event(Event::UserIn);
        }
    }

    pub fn note_off(&mut self, n: u8) {
        self.held[n as usize & 127] = false;
    }

    /// The notes held, low to high (at most 16).
    pub fn held(&self, out: &mut [i32; 16]) -> usize {
        let mut k = 0;
        for (n, &h) in self.held.iter().enumerate() {
            if h && k < 16 {
                out[k] = n as i32;
                k += 1;
            }
        }
        k
    }

    // ---- one block ----

    /// The director's work for one block.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        p: &Params,
        drone: &mut Drone,
        pad: &mut Pad,
        choir: &mut Choir,
        bells: &mut Bells,
        perc: &mut Perc,
        modular: &mut Modular,
        frame: u64,
    ) {
        let dt = BLOCK as f32 / knt_dsp::RATE;
        self.time += dt as f64;
        let any_held = self.held.iter().any(|&h| h);
        if any_held {
            self.idle = 0.0;
        } else {
            self.idle += dt;
        }
        let relay = p.relay_seconds.max(5.0);
        if self.user_active && !any_held && self.idle > relay {
            self.user_active = false;
            self.event(Event::UserOut);
        }
        let leads = self.leads(p);
        // the automations
        let speed = 0.05 + 0.6 * self.pace(p);
        for (w, a) in self.walks.iter_mut().zip(&mut self.auto) {
            w.step(&mut self.rng, dt, speed);
            *a = w.value * p.evolution;
        }
        if !leads {
            // solo, or relay while the user plays: the director's own voices fade out
            pad.release_all();
            if p.mode == Mode::Solo || any_held {
                drone.set_notes(&[]);
                choir.set_notes(&[]);
            }
            self.pulse_tick(p, perc, dt, false);
            return;
        }
        // accompany: the user's chord is the harmony
        if p.mode == Mode::Accompany && any_held {
            let mut v = [0i32; 16];
            let n = self.held(&mut v);
            if let Some(c) = chord_from_played(&v[..n], 60)
                && c.notes() != self.chord.notes()
            {
                self.chord = c;
                let bass = v[0];
                drone.set_notes(&[bass.rem_euclid(12) + 36]);
                choir.set_notes(&self.choir_notes());
            }
        }
        // the chords and sections
        self.chord_left -= dt;
        if self.chord_left <= 0.0 && !(p.hold_harmony && self.chord.len > 0) {
            if self.chords_left == 0 {
                self.new_section(drone, pad, modular);
            }
            self.chords_left = self.chords_left.saturating_sub(1);
            if !(p.mode == Mode::Accompany && any_held) {
                self.new_chord(p, pad, choir, modular);
            }
        } else if self.chord_left <= 0.0 {
            self.chord_left = 5.0;
        }
        // the bells
        self.bell_wait -= dt;
        if self.bell_wait <= 0.0 {
            let density = self.density(p);
            // sparse: one bell every 10 to 60 seconds, fewer while someone plays
            let rate = (0.015 + density * 0.08) * if any_held { 0.4 } else { 1.0 };
            // exponential waits: a Poisson process
            self.bell_wait = -self.rng.f32().max(1e-4).ln() / rate;
            if self.active[Layer::Bells.index()] && p.levels.bells > 0.0 {
                let pool = self.chord.notes();
                let note = if !pool.is_empty() && self.rng.chance(0.7) {
                    pool[self.rng.below(pool.len())] + 12 * (1 + self.rng.below(2) as i32)
                } else {
                    self.scale.note(self.root + 12, self.rng.below(10) as i32)
                };
                let note = note.clamp(64, 96);
                let vel = self.rng.range(0.35, 0.9);
                bells.strike(note, vel, &mut self.rng, frame);
                // sometimes an echoing second bell a fifth or an octave up
                if self.rng.chance(0.04) {
                    let up = if self.rng.chance(0.5) { 7 } else { 12 };
                    bells.strike((note + up).min(100), vel * 0.6, &mut self.rng, frame);
                }
            }
        }
        // the choir's vowel
        self.vowel_wait -= dt;
        if self.vowel_wait <= 0.0 {
            self.vowel_wait = self.rng.range(8.0, 25.0);
            choir.set_vowel(self.rng.weighted(&[3.0, 3.0, 2.0, 1.0, 2.0]));
        }
        self.pulse_tick(p, perc, dt, true);
    }

    /// The pace: the recipe's, moved by the user's bias.
    pub fn pace(&self, p: &Params) -> f32 {
        (self.recipe.pace + p.pace_bias * 0.5).clamp(0.0, 1.0)
    }

    pub fn density(&self, p: &Params) -> f32 {
        (self.recipe.density + p.density_bias * 0.5 + self.auto[5] * 0.25).clamp(0.0, 1.0)
    }

    fn new_section(&mut self, drone: &mut Drone, pad: &mut Pad, modular: &mut Modular) {
        self.section += 1;
        let first = self.section == 1;
        // drift: every third section, another recipe of the Menu
        if self.chosen.drift && !first && self.section % 3 == 1 {
            let mut k = self.rng.below(CARTA.len() - 1);
            if CARTA[k].1.id == self.recipe.id {
                k = (k + 1) % (CARTA.len() - 1);
            }
            let mut r = CARTA[k].1;
            r.drift = false;
            self.recipe = r;
            self.event(Event::Recipe(r.id));
        }
        let r = self.recipe;
        if first || self.rng.chance(r.modulation) || r.root != self.root && self.chosen.drift {
            let old = (self.root, self.scale);
            if !first {
                // to a near key: a fifth, a fourth, a step, or the recipe's own
                let moves = [7, -5, 5, 2, -2, r.root - self.root];
                let m = moves[self.rng.weighted(&[2.0, 2.0, 2.0, 1.0, 1.0, 2.0])];
                self.root = (self.root + m - 50).rem_euclid(12) + 48;
            }
            self.scale = r.scales[self.rng.below(3)];
            if first || old != (self.root, self.scale) {
                self.event(Event::Key {
                    root: self.root.rem_euclid(12),
                    scale: self.scale,
                });
            }
        }
        self.event(Event::Section(self.section));
        self.chords_left = 3 + self.rng.below(4) as u32;
        self.orchestrate(drone, pad, modular);
        // the drone: the root and the fifth, an octave under middle C
        let low = self.root - 12;
        if !self.active[Layer::Drone.index()] {
            drone.set_notes(&[]);
        } else if self.rng.chance(0.3) {
            drone.set_notes(&[low]);
        } else {
            drone.set_notes(&[low, low + 7]);
        }
        // the pulse, now and then; never twice in a row too often
        let pulse = self
            .rng
            .chance(r.pulse * if self.pulse { 0.5 } else { 1.0 });
        if pulse != self.pulse {
            self.pulse = pulse;
            self.event(if pulse {
                Event::PulseIn
            } else {
                Event::PulseOut
            });
            if pulse {
                self.new_pattern();
            }
        }
        let choir = self.rng.chance(r.choir);
        if choir != self.choir {
            self.choir = choir;
            self.event(if choir {
                Event::ChoirIn
            } else {
                Event::ChoirOut
            });
        }
    }

    fn new_chord(&mut self, p: &Params, pad: &mut Pad, choir: &mut Choir, modular: &mut Modular) {
        let n = self.scale.steps().len() as i32;
        let cur = self.chord.degree as i32;
        // favored moves: home, the fourth, the sixth, the second, the fifth
        let targets: [i32; 7] = [0, 3, 5, 1, 4, 2, 6];
        let mut weights = [4.0f32, 3.0, 3.0, 1.5, 1.5, 1.0, 0.5];
        for (t, w) in targets.iter().zip(&mut weights) {
            if t.rem_euclid(n) == cur {
                *w *= 0.15;
            }
        }
        let d = targets[self.rng.weighted(&weights)].rem_euclid(n);
        let density = self.density(p);
        let extra = (density * 2.5 + self.rng.f32() * 0.8) as usize;
        let bright = (self.recipe.brightness + p.brightness_bias * 0.5).clamp(0.0, 1.0);
        // a low register (around G2..D3), lower still when dark
        let center = 46 + (bright * 6.0) as i32;
        let c = Chord::build(self.scale, self.root, d, extra, center);
        self.chord = if self.chord.len > 0 {
            c.led_from(&self.chord)
        } else {
            c
        };
        // half the chords add the chord's root an octave under its lowest note: a deeper pad
        let mut padc = self.chord;
        if self.rng.chance(0.5) && (padc.len as usize) < crate::music::MAX_CHORD {
            let r = self.scale.note(self.root, d);
            let mut low = r;
            while low >= padc.notes()[0] {
                low -= 12;
            }
            let mut v = padc.notes;
            v[padc.len as usize] = low.max(33);
            padc = Chord::from_notes(&v[..padc.len as usize + 1], padc.degree);
        }
        if self.active[Layer::Pad.index()] {
            pad.set_chord(padc.notes());
        } else {
            pad.release_all();
        }
        // the modular plays the chord an octave up
        let mut up = [0i32; crate::music::MAX_CHORD];
        for (u, n) in up.iter_mut().zip(self.chord.notes()) {
            *u = n + 12;
        }
        modular.set_notes(&up[..self.chord.len as usize]);
        let pace = self.pace(p);
        pad.set_times(2.0 + 8.0 * (1.0 - pace), 4.0 + 8.0 * (1.0 - pace));
        if self.choir {
            choir.set_notes(&self.choir_notes());
        } else {
            choir.set_notes(&[]);
        }
        self.chord_len = (36.0 - 27.0 * pace) * self.rng.range(0.75, 1.3);
        self.chord_left = self.chord_len;
        let r = self.scale.note(self.root, d);
        let third = self.scale.note(self.root, d + 2) - r;
        self.event(Event::Chord {
            root: r.rem_euclid(12),
            minor: third.rem_euclid(12) == 3,
        });
    }

    /// The section's orchestration: which of the tonal layers play, and with which timbre. Two
    /// to four of them, never everything at once, so that every section sounds different.
    fn orchestrate(&mut self, drone: &mut Drone, pad: &mut Pad, modular: &mut Modular) {
        let r = self.recipe;
        let lv = r.levels;
        let mut on = [false; LAYERS];
        on[Layer::Drone.index()] = self.rng.chance(0.55 + 0.3 * lv.drone);
        on[Layer::Pad.index()] = self.rng.chance(0.5 + 0.4 * lv.pad);
        on[Layer::Texture.index()] = self.rng.chance(0.25 + 0.6 * lv.texture);
        on[Layer::Bells.index()] = self.rng.chance((lv.bells * 1.4).min(0.6));
        on[Layer::Modular.index()] = self.rng.chance(r.modular);
        // the drone or the pad, at least
        if !on[Layer::Drone.index()] && !on[Layer::Pad.index()] {
            on[if self.rng.chance(0.5) {
                Layer::Drone
            } else {
                Layer::Pad
            }
            .index()] = true;
        }
        // at most four
        let tonal = [
            Layer::Drone,
            Layer::Pad,
            Layer::Texture,
            Layer::Bells,
            Layer::Modular,
        ];
        while tonal.iter().filter(|l| on[l.index()]).count() > 4 {
            let l = tonal[2 + self.rng.below(3)];
            on[l.index()] = false;
        }
        // the timbres
        if on[Layer::Drone.index()] {
            let p = DronePatch::ALL[self.rng.weighted(&[3.0, 2.0, 2.0, 1.5, 1.5])];
            drone.set_patch(p);
        }
        if on[Layer::Pad.index()] {
            let k = self.rng.below(PadPatch::ALL.len());
            let p = if PadPatch::ALL[k] == pad.patch {
                PadPatch::ALL[(k + 1) % PadPatch::ALL.len()]
            } else {
                PadPatch::ALL[k]
            };
            pad.set_patch(p);
        }
        if on[Layer::Modular.index()] {
            let p = if self.rng.chance(0.5) {
                ModularPatch::Sequence
            } else {
                ModularPatch::Bongo
            };
            modular.set_patch(p);
        }
        // the percussion and the choir follow their own sections
        on[Layer::Perc.index()] = true;
        on[Layer::Choir.index()] = true;
        for l in tonal {
            let i = l.index();
            let timbre = match l {
                Layer::Drone => drone.patch.name(),
                Layer::Pad => pad.patch.name(),
                Layer::Modular => modular.patch.name(),
                _ => "",
            };
            if on[i] && (!self.active[i] || !timbre.is_empty()) {
                self.event(Event::Enters(l, timbre));
            } else if !on[i] && self.active[i] {
                self.event(Event::Leaves(l));
            }
        }
        self.active = on;
    }

    /// Up to three notes of the chord in the choir's range.
    fn choir_notes(&self) -> [i32; 3] {
        let mut out = [0i32; 3];
        let notes = self.chord.notes();
        for (k, o) in out.iter_mut().enumerate() {
            let mut n = notes.get(k).copied().unwrap_or(self.root);
            while n < 52 {
                n += 12;
            }
            while n > 67 {
                n -= 12;
            }
            *o = n;
        }
        out
    }

    /// A new percussion pattern: the probability of each sound on each 16th.
    fn new_pattern(&mut self) {
        let mut p = [[0.0f32; 16]; 5];
        p[0][0] = 0.95;
        p[0][8] = self.rng.range(0.0, 0.7);
        if self.rng.chance(0.4) {
            p[0][10] = 0.5;
        }
        for _ in 0..2 + self.rng.below(3) {
            p[1][self.rng.below(16)] = self.rng.range(0.4, 0.9);
        }
        let sh = self.rng.range(0.3, 0.8);
        for s in (0..16).step_by(2) {
            p[2][s] = sh;
        }
        p[3][0] = 0.25;
        for _ in 0..self.rng.below(3) {
            p[4][self.rng.below(16)] = 0.4;
        }
        self.pattern = p;
    }

    fn pulse_tick(&mut self, p: &Params, perc: &mut Perc, dt: f32, leads: bool) {
        if !leads || !self.pulse {
            return;
        }
        self.step_wait -= dt;
        if self.step_wait > 0.0 {
            return;
        }
        let bpm = self.recipe.bpm.clamp(40.0, 140.0);
        self.step_wait += 60.0 / bpm / 4.0;
        let kinds = [Hit::Heart, Hit::Hand, Hit::Shaker, Hit::Bowl, Hit::Click];
        for (k, kind) in kinds.iter().enumerate() {
            let prob = self.pattern[k][self.step];
            // the bowl rings at most once in four bars
            if *kind == Hit::Bowl && !self.bar.is_multiple_of(4) {
                continue;
            }
            if prob > 0.0 && self.rng.chance(prob) {
                let vel = self.rng.range(0.55, 1.0)
                    * if self.step.is_multiple_of(4) {
                        1.0
                    } else {
                        0.7
                    };
                let pitch = self.root + 12;
                perc.hit(*kind, vel, pitch);
            }
        }
        let _ = p;
        self.step = (self.step + 1) % 16;
        if self.step == 0 {
            self.bar += 1;
        }
    }
}
