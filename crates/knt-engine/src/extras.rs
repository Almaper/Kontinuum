//! What sounds beside the music: an ambience of nature (rain, a forest, a fire, the sea), the
//! lo-fi mode (a soft boom-bap beat and the crackle of a record), and binaural tones. All
//! synthesized, nothing recorded, and nothing allocated in the audio callback.

use knt_dsp::{OnePole, RATE, Rng, Svf, coef, note_hz};
use serde::{Deserialize, Serialize};

use crate::BLOCK;
use crate::layers::Block;

/// The ambience under the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Ambience {
    #[default]
    None,
    Rain,
    Forest,
    Fire,
    Sea,
}

impl Ambience {
    pub const ALL: [Ambience; 5] = [
        Ambience::None,
        Ambience::Rain,
        Ambience::Forest,
        Ambience::Fire,
        Ambience::Sea,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Ambience::None => "none",
            Ambience::Rain => "rain",
            Ambience::Forest => "forest",
            Ambience::Fire => "fire",
            Ambience::Sea => "sea",
        }
    }
}

/// A chirp of a bird: a sine whose pitch glides, with a quick envelope.
#[derive(Debug, Clone, Copy, Default)]
struct Chirp {
    phase: f32,
    hz: f32,
    glide: f32,
    amp: f32,
    left: f32,
    pan: f32,
}

/// The nature under the music.
#[derive(Debug, Clone, Default)]
pub struct Nature {
    rng: Rng,
    pink: [f32; 7],
    brown: f32,
    filters: [Svf; 4],
    lows: [OnePole; 4],
    /// The sea's waves and the forest's wind: slow phases.
    swell: f32,
    wind: f32,
    chirps: [Chirp; 3],
    /// The fire's and the rain's single sounds: a click's level, and a drop's resonance.
    click: f32,
    drop: [f32; 2],
    drop_filter: [Svf; 2],
}

impl Nature {
    pub fn new(seed: u64) -> Self {
        Nature {
            rng: Rng::new(seed ^ 0xA3B1),
            ..Default::default()
        }
    }

    fn white(&mut self) -> f32 {
        self.rng.bipolar()
    }

    /// Paul Kellet's pink noise.
    fn pink(&mut self) -> f32 {
        let w = self.white();
        let b = &mut self.pink;
        b[0] = 0.99886 * b[0] + w * 0.0555179;
        b[1] = 0.99332 * b[1] + w * 0.0750759;
        b[2] = 0.96900 * b[2] + w * 0.153852;
        b[3] = 0.86650 * b[3] + w * 0.3104856;
        b[4] = 0.55000 * b[4] + w * 0.5329522;
        b[5] = -0.7616 * b[5] - w * 0.0168980;
        let y = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
        b[6] = w * 0.115926;
        y * 0.11
    }

    /// One block of `kind` at `level` (0..1), added to `out`.
    pub fn render(&mut self, out: &mut Block, kind: Ambience, level: f32) {
        if kind == Ambience::None || level <= 0.0 {
            return;
        }
        let g = level * 1.6;
        let dt = 1.0 / RATE;
        match kind {
            Ambience::Rain => {
                self.filters[0].set(2500.0, 0.1);
                self.filters[1].set(2900.0, 0.1);
                for o in out.iter_mut() {
                    let (a, b) = (self.pink(), self.pink());
                    let l = self.filters[0].process(a).high * 0.7;
                    let r = self.filters[1].process(b).high * 0.7;
                    // drops: a resonant ping now and then, in one ear or the other
                    if self.rng.chance(0.0009) {
                        let side = self.rng.below(2);
                        self.drop[side] = self.rng.range(0.2, 0.6);
                        let hz = self.rng.range(1800.0, 4200.0);
                        self.drop_filter[side].set(hz, 0.93);
                    }
                    let mut d = [0.0; 2];
                    for (s, x) in d.iter_mut().enumerate() {
                        let imp = self.drop[s];
                        self.drop[s] = 0.0;
                        *x = self.drop_filter[s].process(imp).band * 0.6;
                    }
                    o[0] += (l + d[0]) * g;
                    o[1] += (r + d[1]) * g;
                }
            }
            Ambience::Forest => {
                for o in out.iter_mut() {
                    // the wind in the leaves: band-passed noise that swells slowly
                    self.wind = (self.wind + dt / 17.0).fract();
                    let gust =
                        0.35 + 0.65 * (0.5 - 0.5 * (self.wind * std::f32::consts::TAU).cos());
                    self.filters[0].set(500.0 + 900.0 * gust, 0.2);
                    self.filters[1].set(620.0 + 900.0 * gust, 0.2);
                    let (a, b) = (self.pink(), self.pink());
                    let l = self.filters[0].process(a).band * gust * 1.4;
                    let r = self.filters[1].process(b).band * gust * 1.4;
                    // a bird, now and then
                    if self.rng.chance(0.000_12)
                        && let Some(c) = self.chirps.iter_mut().find(|c| c.left <= 0.0)
                    {
                        {
                            *c = Chirp {
                                phase: 0.0,
                                hz: 2400.0 + 2200.0 * self.rng.f32(),
                                glide: self.rng.range(-0.6, 0.9),
                                amp: self.rng.range(0.03, 0.08),
                                left: self.rng.range(0.06, 0.18),
                                pan: self.rng.bipolar() * 0.8,
                            };
                        }
                    }
                    let mut birds = [0.0f32; 2];
                    for c in &mut self.chirps {
                        if c.left > 0.0 {
                            c.left -= dt;
                            c.hz *= 1.0 + c.glide * dt * 4.0;
                            c.phase = (c.phase + c.hz / RATE).fract();
                            // a trill: the amplitude flutters
                            let flutter = 0.6 + 0.4 * (c.left * 180.0).sin();
                            let y = (c.phase * std::f32::consts::TAU).sin() * c.amp * flutter;
                            birds[0] += y * (1.0 - c.pan) * 0.5;
                            birds[1] += y * (1.0 + c.pan) * 0.5;
                        }
                    }
                    o[0] += (l + birds[0]) * g;
                    o[1] += (r + birds[1]) * g;
                }
            }
            Ambience::Fire => {
                let lc = coef(1.0 / (std::f32::consts::TAU * 300.0));
                for o in out.iter_mut() {
                    // the roar: brown noise
                    self.brown = (self.brown + self.white() * 0.02) * 0.998;
                    let roar = self.lows[0].lp(self.brown, lc) * 0.4;
                    // the crackles: sharp clicks in bursts
                    if self.rng.chance(0.0006) {
                        self.click = self.rng.range(0.2, 0.8);
                    }
                    let c = self.white() * self.click;
                    self.click *= 0.97;
                    let hi = self.filters[2].process(c).high;
                    self.filters[2].set(3000.0, 0.3);
                    o[0] += (roar + hi * 0.8) * g;
                    o[1] += (roar * 0.9 + hi * 0.6) * g;
                }
            }
            Ambience::Sea => {
                for o in out.iter_mut() {
                    // a wave every 9 seconds or so: the noise rises, breaks, draws back
                    self.swell = (self.swell + dt / 9.0).fract();
                    let s = self.swell;
                    let env = if s < 0.55 {
                        (s / 0.55).powi(2)
                    } else {
                        (1.0 - (s - 0.55) / 0.45).powf(1.5)
                    };
                    self.filters[0].set(300.0 + 1800.0 * env, 0.05);
                    self.filters[1].set(340.0 + 1800.0 * env, 0.05);
                    let (a, b) = (self.pink(), self.pink());
                    let l = self.filters[0].process(a).low * (0.25 + env) * 0.65;
                    let r = self.filters[1].process(b).low * (0.25 + env) * 0.65;
                    o[0] += l * g;
                    o[1] += r * g;
                }
            }
            Ambience::None => {}
        }
    }
}

/// The lo-fi mode: a soft beat at the recipe's tempo, the crackle of a record, a darker sound.
#[derive(Debug, Clone, Default)]
pub struct Lofi {
    rng: Rng,
    step: usize,
    wait: f32,
    kick: (f32, f32, f32),
    snare: f32,
    hat: f32,
    snare_f: Svf,
    hat_f: Svf,
    crackle: f32,
    crackle_f: Svf,
}

impl Lofi {
    pub fn new(seed: u64) -> Self {
        let mut l = Lofi {
            rng: Rng::new(seed ^ 0x10F1),
            ..Default::default()
        };
        l.snare_f.set(1800.0, 0.3);
        l.hat_f.set(7000.0, 0.2);
        l.crackle_f.set(2500.0, 0.2);
        l
    }

    /// One block of the beat and the crackle, added to `out`.
    pub fn render(&mut self, out: &mut Block, bpm: f32, level: f32) {
        // a lazy tempo: the recipe's, brought into lo-fi's range
        let mut tempo = bpm;
        while tempo > 92.0 {
            tempo /= 2.0;
        }
        while tempo < 64.0 {
            tempo *= 2.0;
        }
        let step_s = 60.0 / tempo / 4.0;
        for o in out.iter_mut() {
            self.wait -= 1.0 / RATE;
            if self.wait <= 0.0 {
                // swing: the off-beats come late
                let swing = if self.step.is_multiple_of(2) {
                    1.12
                } else {
                    0.88
                };
                self.wait += step_s * swing;
                let s = self.step % 16;
                if s == 0 || s == 10 || (s == 7 && self.rng.chance(0.4)) {
                    self.kick = (1.0, 0.0, 130.0);
                }
                if s == 4 || s == 12 {
                    self.snare = 0.6;
                }
                if s.is_multiple_of(2) && self.rng.chance(0.85) {
                    self.hat = if s % 4 == 2 { 0.35 } else { 0.2 };
                }
                self.step += 1;
            }
            // the kick: a sine falling in pitch
            let (amp, phase, hz) = &mut self.kick;
            let mut y = 0.0;
            if *amp > 1e-4 {
                *phase = (*phase + *hz / RATE).fract();
                y += (*phase * std::f32::consts::TAU).sin() * *amp * 0.9;
                *hz = 48.0 + (*hz - 48.0) * 0.9985;
                *amp *= 0.99972;
            }
            // the snare: band-passed noise and a little body
            if self.snare > 1e-4 {
                y += self.snare_f.process(self.rng.bipolar()).band * self.snare * 1.2;
                self.snare *= 0.99955;
            }
            // the hats
            if self.hat > 1e-4 {
                y += self.hat_f.process(self.rng.bipolar()).high * self.hat * 0.35;
                self.hat *= 0.9975;
            }
            // the record: a crackle now and then, a soft hiss always
            if self.rng.chance(0.0004) {
                self.crackle = self.rng.range(0.1, 0.5);
            }
            let cr = self
                .crackle_f
                .process(self.rng.bipolar() * self.crackle)
                .band;
            self.crackle *= 0.9;
            let hiss = self.rng.bipolar() * 0.004;
            let g = level * 0.35;
            o[0] += (y + cr + hiss) * g;
            o[1] += (y * 0.95 + cr * 0.8 + hiss) * g;
        }
    }
}

/// Binaural tones: a low tone in each ear, the right a few hertz higher; the brain hears the
/// difference as a slow beat.
#[derive(Debug, Clone, Copy, Default)]
pub struct Binaural {
    phase: [f32; 2],
}

impl Binaural {
    /// `beat`: the difference in Hz (0: off); `level` 0..1.
    pub fn render(&mut self, out: &mut Block, beat: f32, level: f32) {
        if beat <= 0.0 || level <= 0.0 {
            return;
        }
        let base = note_hz(43.0); // G2, about 98 Hz: low enough to sit under the music
        let hz = [base, base + beat];
        let g = level * 0.06;
        for o in out.iter_mut() {
            for c in 0..2 {
                self.phase[c] = (self.phase[c] + hz[c] / RATE).fract();
                o[c] += (self.phase[c] * std::f32::consts::TAU).sin() * g;
            }
        }
    }
}

/// An empty block.
pub fn silence() -> Block {
    [[0.0; 2]; BLOCK]
}
