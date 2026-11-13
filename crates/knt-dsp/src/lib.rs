//! Kontinuum's DSP building blocks. Everything here works one sample (or one stereo frame) at
//! a time, allocates only when it is made, and is deterministic: the same input and the same
//! seed give the same output on every platform build of the same code.

pub mod fx;
pub mod osc;
pub mod rng;

pub use fx::{Chorus, Delay, Reverb};
pub use osc::{Env, Lfo, OnePole, Osc, Smooth, Svf, Wave};
pub use rng::Rng;

/// The rate everything runs at.
pub const RATE: f32 = 44_100.0;

/// The frequency of MIDI note `n` (A4 = 69 = 440 Hz), fractional notes allowed.
pub fn note_hz(n: f32) -> f32 {
    440.0 * ((n - 69.0) / 12.0).exp2()
}

/// Decibels to a gain factor.
pub fn db(x: f32) -> f32 {
    10f32.powf(x / 20.0)
}

/// The per-sample factor of a one-pole smoother that covers ~63% of a step in `seconds`.
pub fn coef(seconds: f32) -> f32 {
    if seconds <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (seconds * RATE)).exp()
    }
}

/// A soft saturation: about linear under 0.5, never past ±1.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    x / (1.0 + x * x).sqrt()
}

/// An equal-power pan: `p` from -1 (left) to 1 (right); both gains are 1/√2·√2 = 1 at the
/// center.
pub fn pan(p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (
        a.cos() * std::f32::consts::SQRT_2,
        a.sin() * std::f32::consts::SQRT_2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!((note_hz(69.0) - 440.0).abs() < 1e-3);
        assert!((note_hz(57.0) - 220.0).abs() < 1e-3);
        assert!((db(-6.0) - 0.501).abs() < 1e-3);
        let (l, r) = pan(0.0);
        assert!((l - 1.0).abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = pan(-1.0);
        assert!(r.abs() < 1e-6 && l > 1.4);
        assert!(soft_clip(100.0) < 1.0 && soft_clip(-100.0) > -1.0);
        assert!((soft_clip(0.1) - 0.0995).abs() < 1e-3);
    }
}
