//! The time effects: a chorus, a tape-like delay and a long feedback-delay-network reverb.
//! Every buffer is allocated when the effect is made.

use crate::{Lfo, OnePole, RATE, coef};

/// A delay line with a fractional (linearly interpolated) read.
#[derive(Debug, Clone)]
struct Line {
    buf: Box<[f32]>,
    pos: usize,
}

impl Line {
    fn new(len: usize) -> Self {
        Line {
            buf: vec![0.0; len.max(4)].into_boxed_slice(),
            pos: 0,
        }
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }

    /// The value written `d` samples ago (1 ≤ d < len - 1).
    #[inline]
    fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 2) as f32);
        let i = d as usize;
        let f = d - i as f32;
        let a = self.buf[(self.pos + n - i) % n];
        let b = self.buf[(self.pos + n - i - 1) % n];
        a + (b - a) * f
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// A stereo chorus: two slowly modulated short delays, mixed with the dry signal.
#[derive(Debug, Clone)]
pub struct Chorus {
    lines: [Line; 2],
    lfo: [Lfo; 2],
}

impl Default for Chorus {
    fn default() -> Self {
        Self::new()
    }
}

impl Chorus {
    pub fn new() -> Self {
        Chorus {
            lines: [Line::new(2048), Line::new(2048)],
            lfo: [Lfo::new(0.0), Lfo::new(0.25)],
        }
    }

    /// One frame; `mix` 0..1.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, mix: f32) -> (f32, f32) {
        let mut out = [0.0f32; 2];
        for (c, (x, o)) in [l, r].into_iter().zip(&mut out).enumerate() {
            let m = self.lfo[c].step(0.23 + 0.07 * c as f32, 1);
            let d = (0.012 + 0.004 * m) * RATE;
            self.lines[c].write(x);
            let wet = self.lines[c].read(d);
            *o = x * (1.0 - mix * 0.5) + wet * mix * 0.7;
        }
        (out[0], out[1])
    }
}

/// The longest delay, seconds.
pub const MAX_DELAY_S: f32 = 4.0;

/// A stereo ping-pong delay with a tape character: the time glides when it changes (the pitch
/// bends, no click), the repeats darken through a low-pass and saturate softly.
#[derive(Debug, Clone)]
pub struct Delay {
    lines: [Line; 2],
    time: OnePole,
    lp: [OnePole; 2],
    wow: Lfo,
}

impl Default for Delay {
    fn default() -> Self {
        Self::new()
    }
}

impl Delay {
    pub fn new() -> Self {
        let n = (MAX_DELAY_S * RATE) as usize + 8;
        Delay {
            lines: [Line::new(n), Line::new(n)],
            time: OnePole { y: 0.4 * RATE },
            lp: [OnePole::default(); 2],
            wow: Lfo::new(0.0),
        }
    }

    pub fn clear(&mut self) {
        for l in &mut self.lines {
            l.clear();
        }
    }

    /// One frame of the echo alone: `seconds` the time, `feedback` 0..0.95, `tone` 0..1 (dark
    /// to bright).
    #[inline]
    pub fn process(
        &mut self,
        l: f32,
        r: f32,
        seconds: f32,
        feedback: f32,
        tone: f32,
    ) -> (f32, f32) {
        let target = seconds.clamp(0.01, MAX_DELAY_S - 0.01) * RATE;
        let d = self.time.lp(target, 0.00005);
        let wow = self.wow.step(0.4, 1) * 6.0;
        let yl = self.lines[0].read(d + wow);
        let yr = self.lines[1].read(d - wow);
        let c = 0.05 + 0.6 * tone.clamp(0.0, 1.0);
        let fl = self.lp[0].lp(yl, c);
        let fr = self.lp[1].lp(yr, c);
        let fb = feedback.clamp(0.0, 0.95);
        self.lines[0].write(crate::soft_clip(l + fr * fb));
        self.lines[1].write(crate::soft_clip(r + fl * fb));
        (yl, yr)
    }
}

const FDN: usize = 8;
/// The network's line lengths at size 1 (samples at 44.1 kHz; mutually prime).
const LENGTHS: [usize; FDN] = [1427, 1777, 2063, 2311, 2633, 2851, 3209, 3527];
const DIFFUSERS: [usize; 4] = [142, 107, 379, 277];
/// The largest size factor.
const MAX_SIZE: f32 = 2.5;

#[derive(Debug, Clone)]
struct Allpass {
    line: Line,
    len: usize,
}

impl Allpass {
    #[inline]
    fn process(&mut self, x: f32, g: f32) -> f32 {
        let d = self.line.read(self.len as f32);
        let v = x + d * g;
        self.line.write(v);
        d - v * g
    }
}

/// A long, smooth reverb: input diffusers, then an eight-line feedback delay network with a
/// Householder matrix, damping in the loop and slow modulation of two lines.
#[derive(Debug, Clone)]
pub struct Reverb {
    diffusers: Vec<Allpass>,
    lines: Vec<Line>,
    damp: [OnePole; FDN],
    lfo: Lfo,
    size: f32,
    decay: f32,
    gains: [f32; FDN],
    damp_c: f32,
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

impl Reverb {
    pub fn new() -> Self {
        let mut r = Reverb {
            diffusers: DIFFUSERS
                .iter()
                .map(|&n| Allpass {
                    line: Line::new(n + 4),
                    len: n,
                })
                .collect(),
            lines: LENGTHS
                .iter()
                .map(|&n| Line::new((n as f32 * MAX_SIZE) as usize + 32))
                .collect(),
            damp: [OnePole::default(); FDN],
            lfo: Lfo::new(0.0),
            size: 0.0,
            decay: 0.0,
            gains: [0.0; FDN],
            damp_c: 1.0,
        };
        r.set(0.7, 8.0, 0.4);
        r
    }

    /// `size` 0..1 (the lines' length, 0.5x to 2.5x), `decay` the RT60 in seconds, `damp` 0..1
    /// (how quickly the highs die).
    pub fn set(&mut self, size: f32, decay: f32, damp: f32) {
        self.size = 0.5 + 2.0 * size.clamp(0.0, 1.0);
        self.decay = decay.clamp(0.3, 60.0);
        for (g, &n) in self.gains.iter_mut().zip(&LENGTHS) {
            let len = n as f32 * self.size;
            *g = 10f32.powf(-3.0 * len / (self.decay * RATE));
        }
        self.damp_c = coef(0.00002 + 0.0004 * damp.clamp(0.0, 1.0));
    }

    pub fn clear(&mut self) {
        for d in &mut self.diffusers {
            d.line.clear();
        }
        for l in &mut self.lines {
            l.clear();
        }
        self.damp = [OnePole::default(); FDN];
    }

    /// One frame of the wet signal alone.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mut x = (l + r) * 0.5;
        for d in &mut self.diffusers {
            x = d.process(x, 0.7);
        }
        let m = self.lfo.step(0.11, 1) * 12.0;
        let mut y = [0.0f32; FDN];
        for (i, yi) in y.iter_mut().enumerate() {
            let len = LENGTHS[i] as f32 * self.size;
            let mod_ = match i {
                1 => m,
                6 => -m,
                _ => 0.0,
            };
            let v = self.lines[i].read(len + mod_);
            *yi = self.damp[i].lp(v, self.damp_c) * self.gains[i];
        }
        // the Householder reflection: y - (2/N) sum(y)
        let s = y.iter().sum::<f32>() * (2.0 / FDN as f32);
        let side = (l - r) * 0.25;
        for (i, &yi) in y.iter().enumerate() {
            let input = if i % 2 == 0 { x + side } else { x - side };
            self.lines[i].write(yi - s + input * 0.5);
        }
        let mut out = [0.0f32; 2];
        for (i, &yi) in y.iter().enumerate() {
            out[i % 2] += if i % 4 < 2 { yi } else { -yi };
        }
        (out[0] * 0.5, out[1] * 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delay_echoes_after_its_time() {
        let mut d = Delay::new();
        let mut first = None;
        for i in 0..30_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            // the time starts at 0.4 s and is asked to stay there
            let (l, _) = d.process(x, 0.0, 0.4, 0.5, 1.0);
            if l.abs() > 0.1 && first.is_none() {
                first = Some(i);
            }
        }
        let f = first.unwrap() as i64;
        assert!((f - 17_640).abs() < 10, "{f}");
    }

    #[test]
    fn the_reverb_tail_follows_its_decay() {
        for decay in [2.0f32, 8.0] {
            let mut r = Reverb::new();
            r.set(0.6, decay, 0.3);
            let mut energy = [0.0f64; 12];
            for i in 0..12 * 44_100 {
                let x = if i < 32 { 1.0 } else { 0.0 };
                let (a, b) = r.process(x, x);
                energy[i / 44_100] += (a * a + b * b) as f64;
            }
            // RT60: -60 dB in `decay` s, so one second loses 60/decay dB of energy level
            let drop = 10.0 * (energy[2] / energy[3]).log10();
            let want = 60.0 / decay as f64;
            assert!(
                (drop - want).abs() < want * 0.5 + 1.0,
                "decay {decay}: {drop} dB/s"
            );
            assert!(energy.iter().all(|e| e.is_finite()));
        }
        let mut r = Reverb::new();
        assert_eq!(r.process(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn the_chorus_keeps_the_level() {
        let mut c = Chorus::new();
        let mut peak = 0.0f32;
        for i in 0..44_100 {
            let x = (i as f32 * 0.05).sin();
            let (l, _) = c.process(x, x, 0.5);
            peak = peak.max(l.abs());
        }
        assert!(peak > 0.7 && peak < 1.4, "{peak}");
    }
}
