//! A small, fast, seedable random number generator (SplitMix64 to seed, xoshiro128** to run):
//! the director's choices are reproducible from the seed alone.

#[derive(Debug, Clone)]
pub struct Rng {
    s: [u32; 4],
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut next = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let a = next();
        let b = next();
        let mut s = [a as u32, (a >> 32) as u32, b as u32, (b >> 32) as u32];
        if s == [0; 4] {
            s[0] = 1;
        }
        Rng { s }
    }

    pub fn next_u32(&mut self) -> u32 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 9;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(11);
        result
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1).
    pub fn bipolar(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// An integer in [0, n) (0 when n is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            ((self.next_u32() as u64 * n as u64) >> 32) as usize
        }
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// An index chosen with the given weights (the last one when they are all 0).
    pub fn weighted(&mut self, weights: &[f32]) -> usize {
        let total: f32 = weights.iter().map(|w| w.max(0.0)).sum();
        if total <= 0.0 {
            return weights.len().saturating_sub(1);
        }
        let mut x = self.f32() * total;
        for (i, w) in weights.iter().enumerate() {
            x -= w.max(0.0);
            if x < 0.0 {
                return i;
            }
        }
        weights.len() - 1
    }

    /// A rough Gaussian (the sum of four uniforms), mean 0, deviation ~1.
    pub fn gauss(&mut self) -> f32 {
        (self.bipolar() + self.bipolar() + self.bipolar() + self.bipolar()) * 0.866
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_numbers() {
        let a: Vec<u32> = {
            let mut r = Rng::new(7);
            (0..5).map(|_| r.next_u32()).collect()
        };
        let mut r = Rng::new(7);
        let b: Vec<u32> = (0..5).map(|_| r.next_u32()).collect();
        assert_eq!(a, b);
        let mut r = Rng::new(8);
        assert_ne!(a[0], r.next_u32());
    }

    #[test]
    fn ranges() {
        let mut r = Rng::new(1);
        let mut hist = [0usize; 4];
        for _ in 0..40_000 {
            let x = r.f32();
            assert!((0.0..1.0).contains(&x));
            hist[r.below(4)] += 1;
        }
        assert!(
            hist.iter().all(|&n| (9_000..11_000).contains(&n)),
            "{hist:?}"
        );
        let mut w = [0usize; 3];
        for _ in 0..30_000 {
            w[r.weighted(&[1.0, 0.0, 2.0])] += 1;
        }
        assert_eq!(w[1], 0);
        assert!((w[2] as f32 / w[0] as f32 - 2.0).abs() < 0.2);
    }
}
