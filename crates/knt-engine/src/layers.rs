//! The six layers of sound. Each renders a block of [`BLOCK`] stereo frames into its own buffer;
//! the engine mixes them. Nothing here allocates after it is made.

use knt_dsp::{Env, Lfo, OnePole, Osc, RATE, Rng, Svf, Wave, coef, note_hz, pan};

use crate::BLOCK;

pub type Block = [[f32; 2]; BLOCK];

// ---------------------------------------------------------------------------------------------
// the drone

/// The drone's timbres; the director picks one per section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DronePatch {
    /// Clean sines: the fundamental, the octave and a sub, gently saturated. A deep, clean low.
    #[default]
    DeepSine,
    /// Two detuned saws through a four-pole ladder low-pass with a little resonance.
    Ladder,
    /// A two-operator FM tone whose index breathes.
    Fm,
    /// A harmonium-like reed: a narrow pulse with breath.
    Reed,
    /// Noise through three resonators tuned to the notes: a bowed, airy drone.
    Resonant,
}

impl DronePatch {
    pub const ALL: [DronePatch; 5] = [
        DronePatch::DeepSine,
        DronePatch::Ladder,
        DronePatch::Fm,
        DronePatch::Reed,
        DronePatch::Resonant,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DronePatch::DeepSine => "deep sine",
            DronePatch::Ladder => "ladder filter",
            DronePatch::Fm => "FM drone",
            DronePatch::Reed => "reed organ",
            DronePatch::Resonant => "resonant noise",
        }
    }
}

/// A four-pole ladder low-pass (a simplified Moog model with a tanh at its input).
#[derive(Debug, Clone, Copy, Default)]
pub struct Ladder {
    s: [f32; 4],
    g: f32,
    res: f32,
}

impl Ladder {
    pub fn set(&mut self, hz: f32, res: f32) {
        let fc = (hz / RATE).clamp(0.0001, 0.45);
        self.g = 1.0 - (-std::f32::consts::TAU * fc).exp();
        self.res = res.clamp(0.0, 1.0) * 3.6;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = (x - self.res * self.s[3]).tanh();
        self.s[0] += self.g * (x - self.s[0]);
        self.s[1] += self.g * (self.s[0] - self.s[1]);
        self.s[2] += self.g * (self.s[1] - self.s[2]);
        self.s[3] += self.g * (self.s[2] - self.s[3]);
        self.s[3] * (1.0 + self.res * 0.5)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct DroneVoice {
    note: f32,
    env: Env,
    osc: [Osc; 3],
    phase: [f32; 3],
    res: [Svf; 3],
    ladder: Ladder,
    pan: f32,
    patch: DronePatch,
}

/// Long low notes (the key's root and fifth an octave under middle C): one timbre at a time,
/// changing at the sections, through a slowly breathing filter.
pub struct Drone {
    voices: [DroneVoice; 4],
    filter: [Svf; 2],
    lfo: Lfo,
    rng: Rng,
    pub patch: DronePatch,
    /// The current cutoff, Hz (for the stage).
    pub cutoff: f32,
}

impl Drone {
    pub fn new(rng: &mut Rng) -> Self {
        let mut voices = [DroneVoice::default(); 4];
        for (i, v) in voices.iter_mut().enumerate() {
            v.osc = [
                Osc::new(Wave::Saw, rng.f32()),
                Osc::new(Wave::Saw, rng.f32()),
                Osc::new(Wave::Sine, rng.f32()),
            ];
            v.pan = if i % 2 == 0 { -0.3 } else { 0.3 };
            v.env.set_times(10.0, 12.0);
        }
        Drone {
            voices,
            filter: [Svf::default(); 2],
            lfo: Lfo::new(rng.f32()),
            rng: Rng::new(rng.next_u32() as u64),
            patch: DronePatch::DeepSine,
            cutoff: 300.0,
        }
    }

    /// The timbre of the notes that start from now (the sounding ones keep theirs and fade).
    pub fn set_patch(&mut self, p: DronePatch) {
        self.patch = p;
    }

    /// The notes the drone holds: the others fade out, the new ones fade in.
    pub fn set_notes(&mut self, notes: &[i32]) {
        for v in &mut self.voices {
            if v.env.gate && (v.patch != self.patch || !notes.iter().any(|&n| n as f32 == v.note)) {
                v.env.gate = false;
            }
        }
        for &n in notes {
            let n = n as f32;
            if self
                .voices
                .iter()
                .any(|v| v.env.gate && v.note == n && v.patch == self.patch)
            {
                continue;
            }
            let i = quietest(
                self.voices
                    .iter()
                    .map(|v| v.env.value + if v.env.gate { 2.0 } else { 0.0 }),
            );
            let v = &mut self.voices[i];
            v.note = n;
            v.patch = self.patch;
            v.ladder = Ladder::default();
            for r in &mut v.res {
                r.reset();
            }
            v.env.gate = true;
        }
    }

    pub fn render(&mut self, out: &mut Block, brightness: f32, motion: f32) {
        let m = self.lfo.step(0.03 + 0.05 * motion, BLOCK);
        let b = brightness.clamp(0.0, 1.0);
        self.cutoff = 90.0 * (b * 4.5).exp2() * (1.0 + 0.35 * m * motion);
        for f in &mut self.filter {
            f.set(self.cutoff * 2.0, 0.1);
        }
        *out = [[0.0; 2]; BLOCK];
        for v in &mut self.voices {
            let a = v.env.step(BLOCK);
            if a == 0.0 {
                continue;
            }
            let hz = note_hz(v.note);
            let (gl, gr) = pan(v.pan);
            match v.patch {
                DronePatch::Ladder => v.ladder.set(self.cutoff * 1.5, 0.35 + 0.2 * motion),
                DronePatch::Resonant => {
                    for (k, r) in v.res.iter_mut().enumerate() {
                        r.set(hz * [1.0, 2.0, 3.0][k], 0.985);
                    }
                }
                _ => {}
            }
            let index = 0.6 + 0.8 * (0.5 + 0.5 * m) * b;
            for o in out.iter_mut() {
                let s = match v.patch {
                    DronePatch::DeepSine => {
                        let f = v.osc[2].next(hz);
                        let oct = (v.phase[0] * std::f32::consts::TAU).sin();
                        v.phase[0] = (v.phase[0] + 2.0 * hz / RATE).fract();
                        let sub = (v.phase[1] * std::f32::consts::TAU).sin();
                        v.phase[1] = (v.phase[1] + 0.5 * hz / RATE).fract();
                        // a touch of warmth: the even harmonics of a soft clip
                        (f * 0.7 + oct * 0.18 * b + sub * 0.35 + f.abs() * 0.08).tanh()
                    }
                    DronePatch::Ladder => {
                        let x = v.osc[0].next(hz * 1.002) + v.osc[1].next(hz * 0.998);
                        v.ladder.process(x * 0.6) * 1.3
                    }
                    DronePatch::Fm => {
                        let m = (v.phase[0] * std::f32::consts::TAU).sin() * index;
                        v.phase[0] = (v.phase[0] + hz * 2.0 / RATE).fract();
                        let y = ((v.phase[1] + m / std::f32::consts::TAU) * std::f32::consts::TAU)
                            .sin();
                        v.phase[1] = (v.phase[1] + hz / RATE).fract();
                        y * 0.8
                    }
                    DronePatch::Reed => {
                        // a narrow pulse: two saws, one a fifth of a cycle behind
                        let s1 = v.osc[0].next(hz);
                        v.osc[1].phase = (v.osc[0].phase + 0.82).fract();
                        let s2 = v.osc[1].next(hz);
                        let breath = self.rng.bipolar() * 0.03;
                        ((s1 - s2) * 0.4 + breath) * (0.6 + 0.4 * b)
                    }
                    DronePatch::Resonant => {
                        let x = self.rng.bipolar();
                        let mut y = 0.0;
                        for (k, r) in v.res.iter_mut().enumerate() {
                            y += r.process(x).band * [1.0, 0.6, 0.35][k];
                        }
                        y * 0.9
                    }
                };
                o[0] += s * a * gl;
                o[1] += s * a * gr;
            }
        }
        for o in out.iter_mut() {
            o[0] = self.filter[0].process(o[0]).low * 0.42;
            o[1] = self.filter[1].process(o[1]).low * 0.42;
        }
    }
}

fn quietest(levels: impl Iterator<Item = f32>) -> usize {
    let mut best = (0, f32::MAX);
    for (i, l) in levels.enumerate() {
        if l < best.1 {
            best = (i, l);
        }
    }
    best.0
}

// ---------------------------------------------------------------------------------------------
// the pad

/// The pad's timbres; the director picks one per section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadPatch {
    /// The warm chorus pad of the classic polysynths: two saws and a sub.
    #[default]
    Warm,
    /// Analog strings: a pulse whose width sweeps slowly (PWM).
    Strings,
    /// Glass: soft two-operator FM, bell-like at the attack.
    Glass,
    /// A tape organ: additive sines with a little wow.
    Organ,
    /// A modular pad: a triangle through a wavefolder.
    Folded,
    /// Five detuned saws, darkly filtered.
    Supersaw,
    /// Hollow: a square, the clarinet's odd harmonics.
    Hollow,
}

impl PadPatch {
    pub const ALL: [PadPatch; 7] = [
        PadPatch::Warm,
        PadPatch::Strings,
        PadPatch::Glass,
        PadPatch::Organ,
        PadPatch::Folded,
        PadPatch::Supersaw,
        PadPatch::Hollow,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PadPatch::Warm => "warm poly",
            PadPatch::Strings => "PWM strings",
            PadPatch::Glass => "glass FM",
            PadPatch::Organ => "tape organ",
            PadPatch::Folded => "wavefolder",
            PadPatch::Supersaw => "supersaw",
            PadPatch::Hollow => "hollow square",
        }
    }

    /// The filter's opening (relative) and resonance.
    fn filter(self) -> (f32, f32) {
        match self {
            PadPatch::Warm => (1.0, 0.15),
            PadPatch::Strings => (1.4, 0.05),
            PadPatch::Glass => (3.0, 0.0),
            PadPatch::Organ => (2.5, 0.0),
            PadPatch::Folded => (1.6, 0.25),
            PadPatch::Supersaw => (0.7, 0.2),
            PadPatch::Hollow => (1.1, 0.3),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PadVoice {
    note: f32,
    env: Env,
    osc: [Osc; 5],
    phase: [f32; 4],
    filter: Svf,
    pan: f32,
    detune: f32,
    vib: Lfo,
    pwm: Lfo,
    /// Played by hand (a chord change does not release it).
    user: bool,
    vel: f32,
    patch: PadPatch,
}

/// Chords: one timbre at a time ([`PadPatch`]), slow attack and release, a low-pass per voice
/// that blooms with the envelope.
pub struct Pad {
    voices: [PadVoice; 10],
    attack: f32,
    release: f32,
    pub patch: PadPatch,
}

impl Pad {
    pub fn new(rng: &mut Rng) -> Self {
        let mut voices = [PadVoice::default(); 10];
        for v in &mut voices {
            for o in &mut v.osc {
                *o = Osc::new(Wave::Saw, rng.f32());
            }
            v.pan = rng.range(-0.6, 0.6);
            v.detune = rng.range(0.002, 0.006);
            v.vib = Lfo::new(rng.f32());
            v.pwm = Lfo::new(rng.f32());
        }
        Pad {
            voices,
            attack: 4.0,
            release: 7.0,
            patch: PadPatch::Warm,
        }
    }

    pub fn set_patch(&mut self, p: PadPatch) {
        self.patch = p;
    }

    /// Attack and release, seconds.
    pub fn set_times(&mut self, attack: f32, release: f32) {
        self.attack = attack;
        self.release = release;
    }

    fn start(&mut self, n: f32, vel: f32, user: bool) {
        let i = quietest(
            self.voices
                .iter()
                .map(|v| v.env.value + if v.env.gate { 2.0 } else { 0.0 }),
        );
        let v = &mut self.voices[i];
        v.filter.reset();
        v.note = n;
        v.user = user;
        v.vel = vel;
        v.patch = self.patch;
        v.phase = [0.0; 4];
        v.env.set_times(
            if user {
                self.attack * 0.25
            } else {
                self.attack
            },
            self.release,
        );
        v.env.gate = true;
    }

    /// The director's chord: the voices of other notes (or of another timbre) are released (not
    /// those played by hand), the new notes start.
    pub fn set_chord(&mut self, notes: &[i32]) {
        let patch = self.patch;
        for v in &mut self.voices {
            if v.env.gate
                && !v.user
                && (v.patch != patch || !notes.iter().any(|&n| n as f32 == v.note))
            {
                v.env.gate = false;
            }
        }
        for &n in notes {
            let n = n as f32;
            if !self
                .voices
                .iter()
                .any(|v| v.env.gate && v.note == n && v.patch == patch)
            {
                self.start(n, 0.8, false);
            }
        }
    }

    pub fn note_on(&mut self, n: i32, vel: f32) {
        self.start(n as f32, vel, true);
    }

    pub fn note_off(&mut self, n: i32) {
        for v in &mut self.voices {
            if v.user && v.env.gate && v.note == n as f32 {
                v.env.gate = false;
            }
        }
    }

    /// Releases every voice the director started.
    pub fn release_all(&mut self) {
        for v in &mut self.voices {
            if !v.user {
                v.env.gate = false;
            }
        }
    }

    /// How loud the pad is (the voices' envelopes summed), for the stage.
    pub fn activity(&self) -> f32 {
        self.voices.iter().map(|v| v.env.value).sum()
    }

    pub fn render(&mut self, out: &mut Block, brightness: f32, motion: f32) {
        *out = [[0.0; 2]; BLOCK];
        let base = 200.0 * (brightness.clamp(0.0, 1.0) * 4.0).exp2();
        for v in &mut self.voices {
            let a = v.env.step(BLOCK);
            if a == 0.0 {
                continue;
            }
            let vib = v.vib.step(0.15 + motion * 0.3, BLOCK) * 0.002 * (0.3 + motion);
            let hz = note_hz(v.note) * (1.0 + vib);
            let (open, res) = v.patch.filter();
            v.filter.set(base * open * (0.35 + 0.65 * a), res);
            let pw = 0.5 + 0.4 * v.pwm.step(0.11 + 0.2 * motion, BLOCK);
            let (gl, gr) = pan(v.pan);
            let g = a * v.vel * 0.16;
            let d = v.detune;
            for o in out.iter_mut() {
                let s = match v.patch {
                    PadPatch::Warm => {
                        v.osc[0].next(hz * (1.0 + d))
                            + v.osc[1].next(hz * (1.0 - d))
                            + v.osc[2].next(hz * 0.5) * 0.5
                    }
                    PadPatch::Strings => {
                        // two pulses (saw minus a shifted saw), the width sweeping
                        let mut y = 0.0;
                        for (k, det) in [(0usize, 1.0 + d), (2, 1.0 - d)] {
                            let s1 = v.osc[k].next(hz * det);
                            v.osc[k + 1].phase = (v.osc[k].phase + pw).fract();
                            let s2 = v.osc[k + 1].next(hz * det);
                            y += (s1 - s2) * 0.6;
                        }
                        y
                    }
                    PadPatch::Glass => {
                        let m =
                            (v.phase[0] * std::f32::consts::TAU).sin() * (0.4 + 1.6 * (1.0 - a));
                        v.phase[0] = (v.phase[0] + hz * 2.0 / RATE).fract();
                        let y = ((v.phase[1] + m / std::f32::consts::TAU) * std::f32::consts::TAU)
                            .sin();
                        v.phase[1] = (v.phase[1] + hz * (1.0 + d * 0.3) / RATE).fract();
                        y * 1.3
                    }
                    PadPatch::Organ => {
                        let mut y = 0.0;
                        for (k, (h, amp)) in [(1.0, 1.0), (2.0, 0.5), (3.0, 0.3), (4.0, 0.18)]
                            .iter()
                            .enumerate()
                        {
                            y += (v.phase[k] * std::f32::consts::TAU).sin() * amp;
                            v.phase[k] = (v.phase[k] + hz * h / RATE).fract();
                        }
                        y * 0.8
                    }
                    PadPatch::Folded => {
                        v.osc[0].wave = Wave::Triangle;
                        let x = v.osc[0].next(hz);
                        let fold = 1.2 + 2.5 * motion * a;
                        (x * fold * std::f32::consts::FRAC_PI_2).sin() * 0.9
                    }
                    PadPatch::Supersaw => {
                        let mut y = 0.0;
                        for (k, det) in [0.0, -2.0, 2.0, -4.5, 4.5].iter().enumerate() {
                            y += v.osc[k].next(hz * (1.0 + det * d * 0.6));
                        }
                        y * 0.45
                    }
                    PadPatch::Hollow => {
                        let s1 = v.osc[0].next(hz);
                        v.osc[1].phase = (v.osc[0].phase + 0.5).fract();
                        let s2 = v.osc[1].next(hz);
                        (s1 - s2) * 0.55
                    }
                };
                let y = v.filter.process(s).low * g;
                o[0] += y * gl;
                o[1] += y * gr;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the modular

/// What the modular plays; the director picks one per section where it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModularPatch {
    /// A Berlin-school sequence: the chord's notes in a looping 16-step pattern, a resonant
    /// low-pass sweeping slowly.
    #[default]
    Sequence,
    /// West-coast "bongos": sparse plucks through a low-pass gate (amplitude and brightness
    /// decaying together).
    Bongo,
}

impl ModularPatch {
    pub fn name(self) -> &'static str {
        match self {
            ModularPatch::Sequence => "Berlin sequence",
            ModularPatch::Bongo => "low-pass gate plucks",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Pluck {
    osc: Osc,
    modp: f32,
    hz: f32,
    gate: f32,
    decay: f32,
    lpg: f32,
    pan: f32,
    fm: f32,
}

/// The modular synth: a sequencer or a low-pass gate voice, notes from the director's chord.
pub struct Modular {
    voices: [Pluck; 6],
    ladder: [Ladder; 2],
    sweep: Lfo,
    rng: Rng,
    pub patch: ModularPatch,
    /// The sequence: steps (an index into the chord, -1: rest) and where it is.
    pattern: [i8; 16],
    step: usize,
    wait: f32,
    /// The notes it may play.
    notes: [i32; 6],
    len: usize,
    pub played: u32,
}

impl Modular {
    pub fn new(rng: &mut Rng) -> Self {
        let mut m = Modular {
            voices: [Pluck::default(); 6],
            ladder: [Ladder::default(); 2],
            sweep: Lfo::new(rng.f32()),
            rng: Rng::new(rng.next_u32() as u64),
            patch: ModularPatch::Sequence,
            pattern: [-1; 16],
            step: 0,
            wait: 0.0,
            notes: [0; 6],
            len: 0,
            played: 0,
        };
        m.new_pattern();
        m
    }

    pub fn set_patch(&mut self, p: ModularPatch) {
        self.patch = p;
        self.new_pattern();
    }

    /// A new sequence: notes of the chord on about half the steps.
    pub fn new_pattern(&mut self) {
        for s in &mut self.pattern {
            *s = if self.rng.chance(0.55) {
                self.rng.below(6) as i8
            } else {
                -1
            };
        }
        self.pattern[0] = 0;
    }

    pub fn set_notes(&mut self, notes: &[i32]) {
        self.len = notes.len().min(6);
        self.notes[..self.len].copy_from_slice(&notes[..self.len]);
    }

    fn pluck(&mut self, note: i32, vel: f32) {
        let i = quietest(self.voices.iter().map(|v| v.gate));
        let (decay, fm) = match self.patch {
            ModularPatch::Sequence => (self.rng.range(0.15, 0.35), 0.0),
            ModularPatch::Bongo => (self.rng.range(0.25, 0.9), self.rng.range(0.5, 2.5)),
        };
        self.voices[i] = Pluck {
            osc: Osc::new(
                if self.patch == ModularPatch::Sequence {
                    Wave::Saw
                } else {
                    Wave::Triangle
                },
                0.0,
            ),
            modp: 0.0,
            hz: note_hz(note as f32),
            gate: vel,
            decay: 1.0 - coef(decay / 3.0),
            lpg: 1.0,
            pan: self.rng.bipolar() * 0.6,
            fm,
        };
        self.played += 1;
    }

    /// One block: `active` lets it play (the director's orchestration), `bpm` its clock.
    pub fn render(
        &mut self,
        out: &mut Block,
        active: bool,
        bpm: f32,
        brightness: f32,
        motion: f32,
    ) {
        let dt = BLOCK as f32 / RATE;
        if active && self.len > 0 {
            self.wait -= dt;
            if self.wait <= 0.0 {
                match self.patch {
                    ModularPatch::Sequence => {
                        self.wait += 60.0 / bpm.clamp(40.0, 140.0) / 4.0;
                        let s = self.pattern[self.step];
                        if s >= 0 {
                            let n = self.notes[s as usize % self.len];
                            let n = if self.step % 8 == 4 { n + 12 } else { n };
                            let vel = if self.step.is_multiple_of(4) {
                                0.9
                            } else {
                                0.6
                            };
                            self.pluck(n, vel);
                        }
                        self.step = (self.step + 1) % 16;
                        // the sequence mutates a little each time it comes round
                        if self.step == 0 && self.rng.chance(0.3) {
                            let k = self.rng.below(16);
                            self.pattern[k] = if self.pattern[k] < 0 {
                                self.rng.below(6) as i8
                            } else {
                                -1
                            };
                        }
                    }
                    ModularPatch::Bongo => {
                        self.wait = self.rng.range(0.4, 2.5) * (1.2 - motion * 0.6);
                        let n =
                            self.notes[self.rng.below(self.len)] + 12 * self.rng.below(2) as i32;
                        let vel = self.rng.range(0.4, 1.0);
                        self.pluck(n, vel);
                    }
                }
            }
        }
        let sweep = 0.5 + 0.5 * self.sweep.step(0.02 + 0.03 * motion, BLOCK);
        let cut = 300.0 * (brightness.clamp(0.0, 1.0) * 2.0 + sweep * 2.5).exp2();
        for l in &mut self.ladder {
            l.set(cut, 0.55);
        }
        *out = [[0.0; 2]; BLOCK];
        for v in &mut self.voices {
            if v.gate < 1e-4 {
                continue;
            }
            let (gl, gr) = pan(v.pan);
            for o in out.iter_mut() {
                let y = if v.fm > 0.0 {
                    // the low-pass gate: the FM index (its brightness) falls with the level
                    let m = (v.modp * std::f32::consts::TAU).sin() * v.fm * v.lpg;
                    v.modp = (v.modp + v.hz * 1.5 / RATE).fract();
                    let c = v.osc.next(v.hz * (1.0 + m * 0.05));
                    c * v.lpg * v.lpg
                } else {
                    v.osc.next(v.hz) * v.lpg
                };
                let y = y * v.gate * 0.25;
                o[0] += y * gl;
                o[1] += y * gr;
                v.lpg *= v.decay;
            }
            if v.lpg < 1e-3 {
                v.gate = 0.0;
            }
        }
        if self.patch == ModularPatch::Sequence {
            for o in out.iter_mut() {
                o[0] = self.ladder[0].process(o[0]);
                o[1] = self.ladder[1].process(o[1]);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the texture

#[derive(Debug, Clone, Copy, Default)]
struct Drop {
    phase: f32,
    hz: f32,
    amp: f32,
    decay: f32,
    pan: f32,
}

/// Air: pink noise through a slowly sweeping band, and drops (short high blips) at random.
pub struct Texture {
    pink: [[f32; 3]; 2],
    band: [Svf; 2],
    walk: f32,
    lfo: Lfo,
    drops: [Drop; 8],
    rng: Rng,
    /// The band's center, Hz (for the stage).
    pub center: f32,
    /// The drops played (for the stage).
    pub drops_played: u32,
}

impl Texture {
    pub fn new(rng: &mut Rng) -> Self {
        Texture {
            pink: [[0.0; 3]; 2],
            band: [Svf::default(); 2],
            walk: 0.5,
            lfo: Lfo::new(rng.f32()),
            drops: [Drop::default(); 8],
            rng: Rng::new(rng.next_u32() as u64),
            center: 800.0,
            drops_played: 0,
        }
    }

    /// `color` 0..1 moves the band; `rain` 0..1 is how many drops fall.
    pub fn render(&mut self, out: &mut Block, color: f32, motion: f32, rain: f32) {
        self.walk = (self.walk + self.rng.bipolar() * 0.01 * (0.2 + motion)).clamp(0.0, 1.0);
        let m = self.lfo.step(0.02 + 0.04 * motion, BLOCK);
        let c = (color * 0.7 + self.walk * 0.3 + m * 0.1).clamp(0.0, 1.0);
        self.center = 150.0 * (c * 5.5).exp2();
        for b in &mut self.band {
            b.set(self.center, 0.35);
        }
        // drops: a Poisson process, up to ~12 a second
        let p = rain * rain * 12.0 * BLOCK as f32 / RATE;
        if self.rng.chance(p)
            && let Some(d) = self.drops.iter_mut().find(|d| d.amp < 1e-4)
        {
            *d = Drop {
                phase: 0.0,
                hz: self.rng.range(1800.0, 5200.0),
                amp: self.rng.range(0.05, 0.18),
                decay: 1.0 - coef(self.rng.range(0.01, 0.04)),
                pan: self.rng.bipolar() * 0.8,
            };
            self.drops_played += 1;
        }
        for o in out.iter_mut() {
            let mut s = [0.0f32; 2];
            for (c, p) in self.pink.iter_mut().enumerate() {
                // Paul Kellet's economy pink filter
                let w = self.rng.bipolar();
                p[0] = 0.99765 * p[0] + w * 0.0990460;
                p[1] = 0.96300 * p[1] + w * 0.2965164;
                p[2] = 0.57000 * p[2] + w * 1.0526913;
                let pink = (p[0] + p[1] + p[2] + w * 0.1848) * 0.2;
                s[c] = self.band[c].process(pink).band * 0.9 + pink * 0.04;
            }
            *o = s;
        }
        for d in &mut self.drops {
            if d.amp < 1e-4 {
                continue;
            }
            let (gl, gr) = pan(d.pan);
            for o in out.iter_mut() {
                let y = (d.phase * std::f32::consts::TAU).sin() * d.amp;
                d.phase = (d.phase + d.hz / RATE).fract();
                // the drop's pitch falls a little: a water sound
                d.hz *= 0.99995;
                d.amp *= d.decay;
                o[0] += y * gl;
                o[1] += y * gr;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the bells

#[derive(Debug, Clone, Copy, Default)]
struct Bell {
    car: f32,
    modp: f32,
    hz: f32,
    ratio: f32,
    index: f32,
    index_decay: f32,
    amp: f32,
    decay: f32,
    pan: f32,
}

/// Distant bells: two-operator FM with an inharmonic ratio, the brightness fading with the
/// sound.
pub struct Bells {
    voices: [Bell; 10],
    pub played: u32,
    /// The last notes struck and when (frames), for the stage.
    pub recent: [(i32, u64); 8],
    next: usize,
}

impl Bells {
    pub fn new() -> Self {
        Bells {
            voices: [Bell::default(); 10],
            played: 0,
            recent: [(0, 0); 8],
            next: 0,
        }
    }

    pub fn strike(&mut self, note: i32, vel: f32, rng: &mut Rng, now: u64) {
        let i = quietest(self.voices.iter().map(|v| v.amp));
        let ratio = [3.5, 2.0, 1.41, 2.76][rng.below(4)];
        self.voices[i] = Bell {
            car: 0.0,
            modp: 0.0,
            hz: note_hz(note as f32),
            ratio,
            index: rng.range(1.5, 4.0),
            index_decay: 1.0 - coef(rng.range(0.3, 1.2)),
            amp: vel.clamp(0.0, 1.0) * 0.12,
            decay: 1.0 - coef(rng.range(1.5, 4.5)),
            pan: rng.bipolar() * 0.7,
        };
        self.played += 1;
        self.recent[self.next] = (note, now);
        self.next = (self.next + 1) % self.recent.len();
    }

    pub fn render(&mut self, out: &mut Block) {
        *out = [[0.0; 2]; BLOCK];
        for v in &mut self.voices {
            if v.amp < 1e-5 {
                continue;
            }
            let (gl, gr) = pan(v.pan);
            let mi = v.hz * v.ratio / RATE;
            let ci = v.hz / RATE;
            for o in out.iter_mut() {
                let m = (v.modp * std::f32::consts::TAU).sin() * v.index;
                let y = ((v.car + m / std::f32::consts::TAU) * std::f32::consts::TAU).sin() * v.amp;
                v.modp = (v.modp + mi).fract();
                v.car = (v.car + ci).fract();
                v.index *= v.index_decay;
                v.amp *= v.decay;
                o[0] += y * gl;
                o[1] += y * gr;
            }
        }
    }
}

impl Default for Bells {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------------------------
// the percussion

/// The sounds of the percussion: soft and wide, made for a lot of reverb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hit {
    /// A low, soft heartbeat.
    #[default]
    Heart,
    /// A hand drum.
    Hand,
    /// A shaker.
    Shaker,
    /// A singing bowl (long).
    Bowl,
    /// A wooden click.
    Click,
}

impl Hit {
    /// The General MIDI drum notes that play each sound (a drum pad's notes).
    pub fn from_note(n: u8) -> Option<Hit> {
        Some(match n {
            35 | 36 => Hit::Heart,
            37 | 38 | 40 | 41 | 43 | 45 | 47 | 48 | 50 => Hit::Hand,
            39 | 42 | 44 | 46 | 54 | 69 | 70 | 82 => Hit::Shaker,
            49 | 51 | 52 | 53 | 55 | 57 | 59 => Hit::Bowl,
            _ => Hit::Click,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PercVoice {
    kind: Hit,
    t: u32,
    phase: [f32; 3],
    hz: f32,
    amp: f32,
    pan: f32,
    noise: Svf,
}

/// The occasional percussion.
pub struct Perc {
    voices: [PercVoice; 12],
    rng: Rng,
    /// Every hit, for the stage.
    pub played: u32,
    pub last: Hit,
}

impl Perc {
    pub fn new(rng: &mut Rng) -> Self {
        Perc {
            voices: [PercVoice::default(); 12],
            rng: Rng::new(rng.next_u32() as u64),
            played: 0,
            last: Hit::Heart,
        }
    }

    /// Strikes `kind` at velocity `vel`; `pitch` (a MIDI note) tunes the bowl.
    pub fn hit(&mut self, kind: Hit, vel: f32, pitch: i32) {
        let i = quietest(self.voices.iter().map(|v| v.amp));
        let mut noise = Svf::default();
        let (hz, amp) = match kind {
            Hit::Heart => (48.0, 0.55),
            Hit::Hand => (self.rng.range(150.0, 190.0), 0.35),
            Hit::Shaker => {
                noise.set(7000.0, 0.1);
                (0.0, 0.12)
            }
            Hit::Bowl => (note_hz(pitch as f32), 0.12),
            Hit::Click => (self.rng.range(900.0, 1400.0), 0.12),
        };
        if kind == Hit::Hand {
            noise.set(self.rng.range(700.0, 1100.0), 0.4);
        }
        self.voices[i] = PercVoice {
            kind,
            t: 0,
            phase: [0.0; 3],
            hz,
            amp: amp * vel.clamp(0.0, 1.0),
            pan: self.rng.bipolar() * 0.5,
            noise,
        };
        self.played += 1;
        self.last = kind;
    }

    pub fn render(&mut self, out: &mut Block) {
        *out = [[0.0; 2]; BLOCK];
        for v in &mut self.voices {
            if v.amp < 1e-5 {
                continue;
            }
            let (gl, gr) = pan(v.pan);
            for o in out.iter_mut() {
                let t = v.t as f32 / RATE;
                let y = match v.kind {
                    Hit::Heart => {
                        let hz = v.hz + 50.0 * (-t / 0.03).exp();
                        v.phase[0] = (v.phase[0] + hz / RATE).fract();
                        (v.phase[0] * std::f32::consts::TAU).sin() * (-t / 0.35).exp()
                    }
                    Hit::Hand => {
                        let hz = v.hz * (1.0 + 0.15 * (-t / 0.02).exp());
                        v.phase[0] = (v.phase[0] + hz / RATE).fract();
                        let body = (v.phase[0] * std::f32::consts::TAU).sin() * (-t / 0.22).exp();
                        let slap = v.noise.process(self.rng.bipolar()).band * (-t / 0.025).exp();
                        body + slap * 1.5
                    }
                    Hit::Shaker => {
                        let env = (t / 0.012).min(1.0) * (-t / 0.06).exp();
                        v.noise.process(self.rng.bipolar()).high * env
                    }
                    Hit::Bowl => {
                        let mut s = 0.0;
                        for (k, (r, d)) in [(1.0, 7.0), (2.76, 3.5), (5.4, 1.6)].iter().enumerate()
                        {
                            v.phase[k] = (v.phase[k] + v.hz * r / RATE).fract();
                            s += (v.phase[k] * std::f32::consts::TAU).sin()
                                * (-t / d).exp()
                                * (1.0 - 0.3 * k as f32);
                        }
                        s * (t / 0.02).min(1.0)
                    }
                    Hit::Click => {
                        v.phase[0] = (v.phase[0] + v.hz / RATE).fract();
                        (v.phase[0] * std::f32::consts::TAU).sin() * (-t / 0.012).exp()
                    }
                };
                let y = y * v.amp;
                o[0] += y * gl;
                o[1] += y * gr;
                v.t += 1;
            }
            let len = match v.kind {
                Hit::Bowl => 10.0,
                Hit::Heart => 1.5,
                _ => 0.8,
            };
            if v.t as f32 / RATE > len {
                v.amp = 0.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// the choir

/// The vowels the choir sings: the first three formants (Hz) and their gains.
pub const VOWELS: [([f32; 3], [f32; 3]); 5] = [
    ([800.0, 1150.0, 2900.0], [1.0, 0.5, 0.25]),  // a
    ([450.0, 800.0, 2830.0], [1.0, 0.35, 0.15]),  // o
    ([325.0, 700.0, 2530.0], [1.0, 0.25, 0.1]),   // u
    ([400.0, 1600.0, 2700.0], [1.0, 0.35, 0.25]), // e
    ([280.0, 900.0, 2200.0], [1.0, 0.12, 0.05]),  // m (a hum)
];

/// The vowels' names, for the stage.
pub const VOWEL_NAMES: [&str; 5] = ["aah", "ooh", "uuh", "eeh", "mmm"];

#[derive(Debug, Clone, Copy, Default)]
struct ChoirVoice {
    note: f32,
    env: Env,
    osc: Osc,
    vib: Lfo,
    drift: f32,
    formants: [Svf; 3],
    pan: f32,
}

/// A distant choir: saw voices through three formant filters, morphing between vowels.
pub struct Choir {
    voices: [ChoirVoice; 6],
    rng: Rng,
    /// The vowel now (fractional: between two of [`VOWELS`]).
    pub vowel: f32,
    target: f32,
}

impl Choir {
    pub fn new(rng: &mut Rng) -> Self {
        let mut voices = [ChoirVoice::default(); 6];
        for (i, v) in voices.iter_mut().enumerate() {
            v.osc = Osc::new(Wave::Saw, rng.f32());
            v.vib = Lfo::new(rng.f32());
            v.pan = (i as f32 / 5.0) * 1.4 - 0.7;
            v.env.set_times(6.0, 8.0);
        }
        Choir {
            voices,
            rng: Rng::new(rng.next_u32() as u64),
            vowel: 0.0,
            target: 0.0,
        }
    }

    /// The notes the choir sings (from the chord; the others fade).
    pub fn set_notes(&mut self, notes: &[i32]) {
        for v in &mut self.voices {
            if v.env.gate && !notes.iter().any(|&n| n as f32 == v.note) {
                v.env.gate = false;
            }
        }
        for &n in notes.iter().take(self.voices.len()) {
            let n = n as f32;
            if self.voices.iter().any(|v| v.env.gate && v.note == n) {
                continue;
            }
            let i = quietest(
                self.voices
                    .iter()
                    .map(|v| v.env.value + if v.env.gate { 2.0 } else { 0.0 }),
            );
            self.voices[i].note = n;
            self.voices[i].env.gate = true;
        }
    }

    /// Moves toward vowel `v` (an index of [`VOWELS`]).
    pub fn set_vowel(&mut self, v: usize) {
        self.target = v.min(VOWELS.len() - 1) as f32;
    }

    pub fn activity(&self) -> f32 {
        self.voices.iter().map(|v| v.env.value).sum()
    }

    pub fn render(&mut self, out: &mut Block) {
        *out = [[0.0; 2]; BLOCK];
        self.vowel += (self.target - self.vowel) * 0.002;
        let lo = self.vowel.floor() as usize;
        let hi = (lo + 1).min(VOWELS.len() - 1);
        let t = self.vowel - lo as f32;
        let mut f = [0.0f32; 3];
        let mut g = [0.0f32; 3];
        for k in 0..3 {
            f[k] = VOWELS[lo].0[k] + (VOWELS[hi].0[k] - VOWELS[lo].0[k]) * t;
            g[k] = VOWELS[lo].1[k] + (VOWELS[hi].1[k] - VOWELS[lo].1[k]) * t;
        }
        for v in &mut self.voices {
            let a = v.env.step(BLOCK);
            if a == 0.0 {
                continue;
            }
            v.drift = (v.drift + self.rng.bipolar() * 0.0004).clamp(-0.004, 0.004);
            let vib = v.vib.step(4.8, BLOCK) * 0.004;
            let hz = note_hz(v.note) * (1.0 + vib + v.drift);
            for (k, fl) in v.formants.iter_mut().enumerate() {
                fl.set(f[k], 0.75);
            }
            let (gl, gr) = pan(v.pan);
            for o in out.iter_mut() {
                let src = v.osc.next(hz) + self.rng.bipolar() * 0.08;
                let mut y = 0.0;
                for (k, fl) in v.formants.iter_mut().enumerate() {
                    y += fl.process(src).band * g[k];
                }
                let y = y * a * 0.09;
                o[0] += y * gl;
                o[1] += y * gr;
            }
        }
    }
}

/// A slowly moving RMS level of a block (for meters and the stage).
pub fn block_level(b: &Block, meter: &mut OnePole) -> f32 {
    let e = b.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum::<f32>() / (2 * BLOCK) as f32;
    meter.lp(e.sqrt(), 0.08)
}
