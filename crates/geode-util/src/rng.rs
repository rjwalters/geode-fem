//! Tiny deterministic generators for tests and examples that need
//! reproducible pseudo-random data without a `rand` dependency.
//!
//! These replace hand-rolled copies of the same two algorithms that lived in
//! several test files and example binaries. Streams are bit-for-bit
//! compatible with those copies, so golden values do not change.

/// Scale mapping the top 53 bits of a `u64` onto `[0, 1)`.
const UNIT_SCALE: f64 = (1u64 << 53) as f64;

/// Deterministic xorshift64* generator.
///
/// The state is public so callers can seed it verbatim (`Xorshift64Star(seed)`).
/// A zero state is a fixed point of xorshift; use [`Xorshift64Star::new_nonzero`]
/// when the seed may be zero.
#[derive(Clone, Debug)]
pub struct Xorshift64Star(pub u64);

impl Xorshift64Star {
    /// Seed with `seed.max(1)` so a zero seed does not stick at zero.
    pub fn new_nonzero(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// Next raw 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Next sample uniform in `[0, 1)`.
    pub fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / UNIT_SCALE
    }

    /// Next sample uniform in `[-1, 1)`.
    pub fn next_signed(&mut self) -> f64 {
        2.0 * self.next_unit() - 1.0
    }
}

/// Deterministic 64-bit LCG (Knuth MMIX constants).
#[derive(Clone, Debug)]
pub struct Lcg(pub u64);

impl Lcg {
    /// Next sample in `[0, 1)`.
    pub fn next_unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / UNIT_SCALE
    }

    /// Next sample in `[-1, 1)`.
    // Name kept so the many existing test call sites stay unchanged; this is
    // not an `Iterator` (it never terminates and yields `f64` by design).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        self.next_unit() * 2.0 - 1.0
    }

    /// Next sample in `[-0.5, 0.5)`.
    pub fn next_centered(&mut self) -> f64 {
        self.next_unit() - 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xorshift_matches_reference_step() {
        let mut r = Xorshift64Star(1);
        let mut x: u64 = 1;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        assert_eq!(r.next_u64(), x.wrapping_mul(0x2545_F491_4F6C_DD1D));
    }

    #[test]
    fn new_nonzero_clamps_zero_seed() {
        assert_eq!(Xorshift64Star::new_nonzero(0).0, 1);
        assert_eq!(Xorshift64Star::new_nonzero(7).0, 7);
    }

    #[test]
    fn lcg_matches_reference_step() {
        let mut r = Lcg(3);
        let s = 3u64
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        assert_eq!(r.next_unit(), (s >> 11) as f64 / (1u64 << 53) as f64);
    }

    #[test]
    fn samples_stay_in_range() {
        let mut x = Xorshift64Star(42);
        let mut l = Lcg(42);
        for _ in 0..1000 {
            assert!((0.0..1.0).contains(&x.next_unit()));
            assert!((-1.0..1.0).contains(&x.next_signed()));
            assert!((-1.0..1.0).contains(&l.next()));
            assert!((-0.5..0.5).contains(&l.next_centered()));
        }
    }
}
