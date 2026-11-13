//! Oscillators, envelopes, LFOs, smoothers and filters.

use crate::RATE;

/// An oscillator's waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    /// A band-limited saw (polyBLEP).
    Saw,
}

/// The polyBLEP correction at phase `t` for a phase step `dt`.
#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// An oscillator: a phase in [0, 1) and a waveform.
#[derive(Debug, Clone, Copy, Default)]
pub struct Osc {
    pub phase: f32,
    pub wave: Wave,
}

impl Osc {
    pub fn new(wave: Wave, phase: f32) -> Self {
        Osc {
            phase: phase.rem_euclid(1.0),
            wave,
        }
    }

    /// One sample at `hz`.
    #[inline]
    pub fn next(&mut self, hz: f32) -> f32 {
        let dt = (hz / RATE).clamp(0.0, 0.45);
        let p = self.phase;
        let y = match self.wave {
            Wave::Sine => (p * std::f32::consts::TAU).sin(),
            Wave::Triangle => {
                let x = 4.0 * p;
                if p < 0.25 {
                    x
                } else if p < 0.75 {
                    2.0 - x
                } else {
                    x - 4.0
                }
            }
            Wave::Saw => 2.0 * p - 1.0 - blep(p, dt),
        };
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

/// A slow sine LFO, 0-centered, amplitude 1.
#[derive(Debug, Clone, Copy, Default)]
pub struct Lfo {
    pub phase: f32,
}

impl Lfo {
    pub fn new(phase: f32) -> Self {
        Lfo { phase }
    }

    /// Advances by `n` samples at `hz`; returns the value.
    #[inline]
    pub fn step(&mut self, hz: f32, n: usize) -> f32 {
        self.phase = (self.phase + hz * n as f32 / RATE).rem_euclid(1.0);
        (self.phase * std::f32::consts::TAU).sin()
    }
}

/// An attack-release envelope with exponential segments: it rises toward 1 while the gate is
/// on and falls toward 0 when it is off. Times are seconds to ~95%.
#[derive(Debug, Clone, Copy, Default)]
pub struct Env {
    pub value: f32,
    pub gate: bool,
    up: f32,
    down: f32,
}

impl Env {
    pub fn set_times(&mut self, attack: f32, release: f32) {
        self.up = crate::coef(attack.max(0.001) / 3.0);
        self.down = crate::coef(release.max(0.001) / 3.0);
    }

    /// One step of `n` samples (the envelope runs at control rate: per block).
    #[inline]
    pub fn step(&mut self, n: usize) -> f32 {
        let (target, c) = if self.gate {
            (1.05, self.up)
        } else {
            (0.0, self.down)
        };
        let c = 1.0 - (1.0 - c).powi(n as i32);
        self.value += (target - self.value) * c;
        self.value = self.value.min(1.0);
        if !self.gate && self.value < 1e-5 {
            self.value = 0.0;
        }
        self.value
    }

    pub fn idle(&self) -> bool {
        !self.gate && self.value == 0.0
    }
}

/// A one-pole low-pass (also a parameter smoother).
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    pub y: f32,
}

impl OnePole {
    #[inline]
    pub fn lp(&mut self, x: f32, c: f32) -> f32 {
        self.y += (x - self.y) * c;
        self.y
    }
}

/// A smoothed parameter: `get` moves toward the target with a time constant.
#[derive(Debug, Clone, Copy)]
pub struct Smooth {
    pub value: f32,
    pub target: f32,
    c: f32,
}

impl Smooth {
    pub fn new(v: f32, seconds: f32) -> Self {
        Smooth {
            value: v,
            target: v,
            c: crate::coef(seconds),
        }
    }

    /// One control step of `n` samples.
    #[inline]
    pub fn step(&mut self, n: usize) -> f32 {
        let c = 1.0 - (1.0 - self.c).powi(n as i32);
        self.value += (self.target - self.value) * c;
        self.value
    }
}

/// A state-variable filter (topology-preserving transform): low-, band- and high-pass at once.
#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

/// The outputs of one [`Svf`] step.
#[derive(Debug, Clone, Copy)]
pub struct SvfOut {
    pub low: f32,
    pub band: f32,
    pub high: f32,
}

impl Svf {
    /// Sets the cutoff (Hz) and the resonance (0..1).
    pub fn set(&mut self, hz: f32, resonance: f32) {
        let fc = hz.clamp(10.0, RATE * 0.45);
        let g = (std::f32::consts::PI * fc / RATE).tan();
        let k = 2.0 - 1.9 * resonance.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        self.a1 = a1;
        self.a2 = g * a1;
        self.a3 = g * g * a1;
        self.k = k;
    }

    #[inline]
    pub fn process(&mut self, v0: f32) -> SvfOut {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut {
            low: v2,
            band: v1,
            high: v0 - self.k * v1 - v2,
        }
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oscillators_are_bounded_and_periodic() {
        for wave in [Wave::Sine, Wave::Triangle, Wave::Saw] {
            let mut o = Osc::new(wave, 0.0);
            let mut sum = 0.0f64;
            let mut peak = 0.0f32;
            for _ in 0..44_100 {
                let y = o.next(441.0);
                sum += y as f64;
                peak = peak.max(y.abs());
            }
            assert!(peak <= 1.1 && peak > 0.9, "{wave:?} {peak}");
            assert!(
                (sum / 44_100.0).abs() < 0.01,
                "{wave:?} dc {}",
                sum / 44_100.0
            );
        }
    }

    #[test]
    fn the_envelope_rises_and_falls() {
        let mut e = Env::default();
        e.set_times(1.0, 2.0);
        e.gate = true;
        // one second in 64-sample blocks
        for _ in 0..689 {
            e.step(64);
        }
        assert!(e.value > 0.9, "{}", e.value);
        e.gate = false;
        for _ in 0..689 {
            e.step(64);
        }
        assert!(e.value < 0.3 && e.value > 0.1, "{}", e.value);
        for _ in 0..5000 {
            e.step(64);
        }
        assert!(e.idle());
    }

    #[test]
    fn the_filter_cuts() {
        let mut f = Svf::default();
        f.set(500.0, 0.0);
        let mut o = Osc::new(Wave::Sine, 0.0);
        let mut peak = 0.0f32;
        for i in 0..44_100 {
            let y = f.process(o.next(8000.0)).low;
            if i > 4410 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.01, "{peak}");
    }
}
