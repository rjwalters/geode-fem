//! Crate-internal xorshift64* generator for deterministic start vectors.
//!
//! `geode-util` has the same algorithm but depends on `geode-core`, so
//! library code here cannot use it. Streams are bit-identical to the
//! hand-rolled copies this replaces.

/// Deterministic xorshift64* generator (non-zero state).
pub(crate) struct Xorshift64Star(u64);

impl Xorshift64Star {
    /// Seed with `seed.max(1)` so a zero seed does not stick at zero.
    pub(crate) fn new_nonzero(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// Seed with `seed · 0x9E37_79B9_7F4A_7C15 | 1` (Fibonacci hash, forced odd).
    pub(crate) fn hashed(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// Next sample uniform in `[0, 1)`.
    pub(crate) fn next_unit(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (r >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Next sample uniform in `[-1, 1)`.
    pub(crate) fn next_signed(&mut self) -> f64 {
        self.next_unit() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_step() {
        let mut r = Xorshift64Star::new_nonzero(0);
        let mut x: u64 = 1;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        let u = x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        assert_eq!(r.next_unit(), u as f64 / (1u64 << 53) as f64);
        let h = Xorshift64Star::hashed(3);
        assert_eq!(h.0, 3u64.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    }
}
