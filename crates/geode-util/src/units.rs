//! Unit conversions for the FEM / electromagnetics stack.

/// Angular frequency `ω = 2π f` for `f` in GHz, expressed in the natural
/// (inverse-length) unit of the mesh — i.e. divided by the speed of light
/// in that length unit.
///
/// Pass `c_per_unit` as the matching `geode_core::constants::C_*_PER_S`
/// (`C_MM_PER_S` for millimeter meshes, `C_UM_PER_S` for micrometer
/// meshes). Centralizes the `ghz_to_omega` helper that was copy-pasted
/// across the example binaries and the patch / spiral reference tests —
/// where the divisor silently differed (mm vs µm) between call sites.
pub fn ghz_to_omega(f_ghz: f64, c_per_unit: f64) -> f64 {
    2.0 * std::f64::consts::PI * f_ghz * 1.0e9 / c_per_unit
}

/// [`ghz_to_omega`] for **millimeter** meshes (divisor
/// [`geode_core::constants::C_MM_PER_S`]).
pub fn ghz_to_omega_mm(f_ghz: f64) -> f64 {
    ghz_to_omega(f_ghz, geode_core::constants::C_MM_PER_S)
}

/// [`ghz_to_omega`] for **micrometer** meshes (divisor
/// [`geode_core::constants::C_UM_PER_S`]).
pub fn ghz_to_omega_um(f_ghz: f64) -> f64 {
    ghz_to_omega(f_ghz, geode_core::constants::C_UM_PER_S)
}

/// Inverse of [`ghz_to_omega`]: frequency in GHz from a natural-unit
/// angular frequency `ω` and the speed of light `c_per_unit` in the mesh's
/// length unit. Centralizes the `ω · c / (2π · 1e9)` SRF back-conversion
/// duplicated in the spiral / slcfet example binaries.
pub fn omega_to_ghz(omega: f64, c_per_unit: f64) -> f64 {
    omega * c_per_unit / (2.0 * std::f64::consts::PI * 1.0e9)
}

/// [`omega_to_ghz`] for **millimeter** meshes.
pub fn omega_to_ghz_mm(omega: f64) -> f64 {
    omega_to_ghz(omega, geode_core::constants::C_MM_PER_S)
}

/// [`omega_to_ghz`] for **micrometer** meshes.
pub fn omega_to_ghz_um(omega: f64) -> f64 {
    omega_to_ghz(omega, geode_core::constants::C_UM_PER_S)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C_MM_PER_S: f64 = 2.997_924_58e11;
    const C_UM_PER_S: f64 = 2.997_924_58e14;

    #[test]
    fn ghz_to_omega_matches_closed_form() {
        let w = ghz_to_omega(2.4, C_MM_PER_S);
        let expected = 2.0 * std::f64::consts::PI * 2.4 * 1.0e9 / C_MM_PER_S;
        assert!((w - expected).abs() < 1e-15);
    }

    #[test]
    fn ghz_to_omega_scales_inversely_with_length_unit() {
        // µm `c` is 1000× the mm `c`, so ω in µm-units is 1000× smaller.
        let w_mm = ghz_to_omega(1.0, C_MM_PER_S);
        let w_um = ghz_to_omega(1.0, C_UM_PER_S);
        assert!((w_um * 1000.0 - w_mm).abs() < 1e-12);
    }

    #[test]
    fn omega_to_ghz_is_inverse_of_ghz_to_omega() {
        let f = 2.4;
        let round_trip = omega_to_ghz(ghz_to_omega(f, C_MM_PER_S), C_MM_PER_S);
        assert!((round_trip - f).abs() < 1e-12);
    }

    // The convenience wrappers bind `geode_core::constants` — assert they
    // pick the *correct* length-unit `c` (the mm/µm mix-up this module
    // exists to prevent), comparing against the canonical core constants.

    #[test]
    fn ghz_to_omega_mm_binds_millimeter_c() {
        let f = 2.4;
        assert_eq!(
            ghz_to_omega_mm(f),
            ghz_to_omega(f, geode_core::constants::C_MM_PER_S)
        );
    }

    #[test]
    fn ghz_to_omega_um_binds_micrometer_c() {
        let f = 2.4;
        assert_eq!(
            ghz_to_omega_um(f),
            ghz_to_omega(f, geode_core::constants::C_UM_PER_S)
        );
    }

    #[test]
    fn mm_and_um_wrappers_differ_by_the_unit_ratio() {
        // ω_mm = 1000 · ω_µm — a wrapper that bound the wrong constant
        // (the silent bug the unit split guards against) would fail here.
        assert!((ghz_to_omega_mm(1.0) - 1000.0 * ghz_to_omega_um(1.0)).abs() < 1e-12);
    }

    #[test]
    fn omega_to_ghz_wrappers_invert_their_forward_partners() {
        let f = 3.7;
        assert!((omega_to_ghz_mm(ghz_to_omega_mm(f)) - f).abs() < 1e-12);
        assert!((omega_to_ghz_um(ghz_to_omega_um(f)) - f).abs() < 1e-12);
    }
}
