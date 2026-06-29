//! Language-interop reference decoders.
//!
//! Staging home (Epic #414, Phase 2) for decoders that read cross-language
//! reference payloads (JAX / Julia / TF-Java / NumPy real-imag splits) and
//! reassemble them into complex values for cross-backend validation.
//!
//! The cross-language reference oracles serialize complex arrays as a flat
//! `f64` buffer in **real-imag interleaved** order — `[re₀, im₀, re₁, im₁, …]`
//! — because most of them lack a portable native complex on-disk dtype. This
//! module owns the single, tested reassembly of that layout into
//! [`Complex64`], replacing the per-test ad-hoc `chunks_exact(2)` /
//! `Complex64::new(re, im)` idioms that used to live inside the
//! `geode-validation` reference suite.

use num_complex::Complex64;

/// Decode a real-imag interleaved `f64` payload into a row-major
/// `Vec<Complex64>`.
///
/// The on-disk encoding is `[re₀, im₀, re₁, im₁, …]`; each adjacent pair
/// becomes one `Complex64`. This is the canonical reassembly used by the
/// fixture `c128` decode path and the reference drivers.
///
/// `flat.len()` is expected to be even (`2 × <complex count>`). A trailing
/// unpaired `f64` cannot form a complex value and is silently dropped — the
/// fixture loader validates the declared length up front, and callers that
/// must reject a malformed payload should use
/// [`decode_real_imag_interleave_exact`].
#[must_use]
pub fn decode_real_imag_interleave(flat: &[f64]) -> Vec<Complex64> {
    flat.chunks_exact(2)
        .map(|pair| Complex64::new(pair[0], pair[1]))
        .collect()
}

/// Length-checked variant of [`decode_real_imag_interleave`].
///
/// Returns `Some` only when `flat.len() == 2 * expected_len`, i.e. the
/// payload holds exactly `expected_len` interleaved complex values; returns
/// `None` otherwise (wrong length, including an odd-length / truncated
/// payload). This lets schema-aware callers map the length mismatch onto
/// their own domain error while keeping the interleave arithmetic here.
#[must_use]
pub fn decode_real_imag_interleave_exact(
    flat: &[f64],
    expected_len: usize,
) -> Option<Vec<Complex64>> {
    if flat.len() != 2 * expected_len {
        return None;
    }
    Some(decode_real_imag_interleave(flat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_interleaved_pairs() {
        let flat = [1.0, 2.0, 3.0, -4.0];
        let got = decode_real_imag_interleave(&flat);
        assert_eq!(
            got,
            vec![Complex64::new(1.0, 2.0), Complex64::new(3.0, -4.0)]
        );
    }

    #[test]
    fn decodes_empty_to_empty() {
        assert!(decode_real_imag_interleave(&[]).is_empty());
    }

    #[test]
    fn drops_trailing_unpaired_value() {
        // Odd length: the final lone `re` has no `im` partner and is dropped.
        let got = decode_real_imag_interleave(&[1.0, 2.0, 3.0]);
        assert_eq!(got, vec![Complex64::new(1.0, 2.0)]);
    }

    #[test]
    fn exact_accepts_matching_length() {
        let flat = [0.5, -0.5, 7.0, 0.0];
        let got = decode_real_imag_interleave_exact(&flat, 2).expect("len matches 2 complex");
        assert_eq!(got.len(), 2);
        assert_eq!(got[1], Complex64::new(7.0, 0.0));
    }

    #[test]
    fn exact_rejects_wrong_length() {
        // 4 f64 = 2 complex, but caller asked for 3.
        assert!(decode_real_imag_interleave_exact(&[1.0, 2.0, 3.0, 4.0], 3).is_none());
        // Odd-length payload can never satisfy any `expected_len`.
        assert!(decode_real_imag_interleave_exact(&[1.0, 2.0, 3.0], 1).is_none());
        // Zero-length round-trips at expected_len 0.
        assert!(decode_real_imag_interleave_exact(&[], 0).is_some());
    }
}
