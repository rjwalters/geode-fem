//! Mohan analytic inductance expressions for square planar spiral
//! inductors (Epic #193 Phase 3, issue #211).
//!
//! Implements the three closed-form low-frequency inductance
//! expressions of Mohan, del Mar Hershenson, Boyd & Lee, *Simple
//! Accurate Expressions for Planar Spiral Inductances*, IEEE JSSC
//! 34(10), Oct 1999, specialized to the **square** layout of the
//! bundled spiral fixture (`geode_core::mesh::spiral`):
//!
//! 1. **Current-sheet approximation** ([`mohan_current_sheet_l`]) —
//!    the paper's Eq. (5) with the square coefficients
//!    `c = (1.27, 2.07, 0.18, 0.13)`;
//! 2. **Modified Wheeler** ([`modified_wheeler_l`]) — Eq. (2) with
//!    square `K₁ = 2.34, K₂ = 2.75`;
//! 3. **Data-fitted monomial** ([`monomial_fit_l`]) — Eq. (6) with the
//!    square table `β = 1.62e-3`,
//!    `α = (−1.21, −0.147, 2.40, 1.78, −0.030)`.
//!
//! All three take the standard spiral parameterization: turn count `n`
//! (fractional turns allowed — the current-sheet derivation treats the
//! winding as a uniform sheet, so `n` enters only through `n²` and the
//! outer diameter), trace width `w`, turn spacing `s`, and inner
//! diameter `d_in`, with
//!
//! ```text
//! d_out = d_in + 2·(n·w + (n − 1)·s)
//! d_avg = (d_out + d_in) / 2
//! ρ     = (d_out − d_in) / (d_out + d_in)      (fill ratio)
//! ```
//!
//! The three expressions are derived independently (field analysis of a
//! current sheet, a refit of Wheeler's 1928 formula, and a least-squares
//! monomial fit to a field-solver dataset); the paper reports that they
//! agree with each other and with field-solver/measured values to
//! ±~5–8 % over practical geometries. They are an **analytic sanity
//! oracle** for the FEM benchmark, not a 5 %-grade reference: they
//! assume no ground plane (the FEM fixture has PEC walls ~50 µm below
//! the spiral, which reduces L via image currents), no feed stubs, and
//! quasi-static low-frequency operation.
//!
//! Lengths are in **meters** and the returned inductance in **henries**
//! ([`monomial_fit_l`] converts internally to the µm/nH units of the
//! published fit).

/// Vacuum permeability `μ₀` (H/m).
const MU_0: f64 = 4.0e-7 * std::f64::consts::PI;

/// Geometric parameters of a square planar spiral (lengths in meters).
#[derive(Debug, Clone, Copy)]
pub struct SquareSpiral {
    /// Number of turns (fractional allowed).
    pub n_turns: f64,
    /// Trace width `w`.
    pub width: f64,
    /// Turn-to-turn spacing `s`.
    pub spacing: f64,
    /// Inner diameter `d_in` (inner edge to inner edge).
    pub d_in: f64,
}

impl SquareSpiral {
    /// Outer diameter `d_out = d_in + 2(n·w + (n−1)·s)`.
    pub fn d_out(&self) -> f64 {
        self.d_in + 2.0 * (self.n_turns * self.width + (self.n_turns - 1.0) * self.spacing)
    }

    /// Average diameter `d_avg = (d_out + d_in)/2`.
    pub fn d_avg(&self) -> f64 {
        0.5 * (self.d_out() + self.d_in)
    }

    /// Fill ratio `ρ = (d_out − d_in)/(d_out + d_in)`.
    pub fn fill_ratio(&self) -> f64 {
        let d_out = self.d_out();
        (d_out - self.d_in) / (d_out + self.d_in)
    }
}

/// Current-sheet inductance (Mohan Eq. (5), square layout):
///
/// ```text
/// L = (μ₀ n² d_avg c₁ / 2) · [ln(c₂/ρ) + c₃ρ + c₄ρ²],
/// (c₁, c₂, c₃, c₄) = (1.27, 2.07, 0.18, 0.13).
/// ```
///
/// Lengths in meters, result in henries.
pub fn mohan_current_sheet_l(s: &SquareSpiral) -> f64 {
    const C1: f64 = 1.27;
    const C2: f64 = 2.07;
    const C3: f64 = 0.18;
    const C4: f64 = 0.13;
    let rho = s.fill_ratio();
    0.5 * MU_0
        * s.n_turns
        * s.n_turns
        * s.d_avg()
        * C1
        * ((C2 / rho).ln() + C3 * rho + C4 * rho * rho)
}

/// Modified-Wheeler inductance (Mohan Eq. (2), square layout):
///
/// ```text
/// L = K₁ μ₀ n² d_avg / (1 + K₂ ρ),   (K₁, K₂) = (2.34, 2.75).
/// ```
///
/// Lengths in meters, result in henries.
pub fn modified_wheeler_l(s: &SquareSpiral) -> f64 {
    const K1: f64 = 2.34;
    const K2: f64 = 2.75;
    K1 * MU_0 * s.n_turns * s.n_turns * s.d_avg() / (1.0 + K2 * s.fill_ratio())
}

/// Data-fitted monomial inductance (Mohan Eq. (6), square layout):
///
/// ```text
/// L[nH] = β d_out^α₁ w^α₂ d_avg^α₃ n^α₄ s^α₅,
/// β = 1.62e-3,  α = (−1.21, −0.147, 2.40, 1.78, −0.030),
/// ```
///
/// with the published fit's µm/nH units handled internally: lengths in
/// meters, result in henries.
pub fn monomial_fit_l(s: &SquareSpiral) -> f64 {
    const BETA: f64 = 1.62e-3;
    const A1: f64 = -1.21; // d_out
    const A2: f64 = -0.147; // w
    const A3: f64 = 2.40; // d_avg
    const A4: f64 = 1.78; // n
    const A5: f64 = -0.030; // s
    let um = 1.0e6;
    let l_nh = BETA
        * (s.d_out() * um).powf(A1)
        * (s.width * um).powf(A2)
        * (s.d_avg() * um).powf(A3)
        * s.n_turns.powf(A4)
        * (s.spacing * um).powf(A5);
    l_nh * 1.0e-9
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled fixture geometry
    /// (`tests/fixtures/spiral_3p5.provenance.txt`): 3.5 turns,
    /// w = 6 µm, s = 4 µm, d_in = 60 µm.
    fn fixture_spiral() -> SquareSpiral {
        SquareSpiral {
            n_turns: 3.5,
            width: 6.0e-6,
            spacing: 4.0e-6,
            d_in: 60.0e-6,
        }
    }

    /// Derived geometry of the fixture spiral:
    /// `d_out = 60 + 2(3.5·6 + 2.5·4) = 122 µm`, `d_avg = 91 µm`,
    /// `ρ = 62/182`.
    #[test]
    fn fixture_derived_geometry() {
        let s = fixture_spiral();
        assert!((s.d_out() - 122.0e-6).abs() < 1e-12);
        assert!((s.d_avg() - 91.0e-6).abs() < 1e-12);
        assert!((s.fill_ratio() - 62.0 / 182.0).abs() < 1e-12);
    }

    /// The three independently derived expressions (field-analysis
    /// current sheet, refit Wheeler, data-fitted monomial) must agree
    /// pairwise to a few percent on the fixture geometry — the paper's
    /// own cross-consistency claim (its exhaustive comparison reports
    /// the three within ~2-3 % of each other over practical layouts).
    #[test]
    fn three_expressions_mutually_consistent() {
        let s = fixture_spiral();
        let l_cs = mohan_current_sheet_l(&s);
        let l_mw = modified_wheeler_l(&s);
        let l_mono = monomial_fit_l(&s);
        for (name, a, b) in [
            ("current-sheet vs wheeler", l_cs, l_mw),
            ("current-sheet vs monomial", l_cs, l_mono),
            ("wheeler vs monomial", l_mw, l_mono),
        ] {
            let rel = (a - b).abs() / a;
            assert!(
                rel < 0.05,
                "{name}: {a:.4e} H vs {b:.4e} H differ by {:.2}%",
                100.0 * rel
            );
        }
    }

    /// Pinned current-sheet value on the fixture geometry: hand
    /// evaluation of Eq. (5) gives
    /// `L = 0.635 μ₀ · 3.5² · 91 µm · [ln(2.07/ρ) + 0.18ρ + 0.13ρ²]
    ///    = 1.6731 nH` for `ρ = 62/182`. Regression-pins the value the
    /// FEM benchmark (`benchmarks/spiral_inductor/`) quotes as its
    /// analytic oracle.
    #[test]
    fn current_sheet_fixture_value_pinned() {
        let l = mohan_current_sheet_l(&fixture_spiral());
        let l_ref = 1.6731e-9;
        assert!(
            (l - l_ref).abs() / l_ref < 1e-3,
            "current-sheet L = {l:.6e} H, expected {l_ref:.4e} H"
        );
    }

    /// Physical scalings of the current-sheet expression: more turns
    /// and larger average diameter both increase L; at fixed d_in,
    /// doubling n roughly quadruples n² but also grows ρ, so L grows
    /// strictly but sub-quadratically times the log correction — just
    /// assert strict monotonicity in n and d_in.
    #[test]
    fn current_sheet_monotone_in_turns_and_diameter() {
        let base = fixture_spiral();
        let mut more_turns = base;
        more_turns.n_turns = 4.5;
        assert!(mohan_current_sheet_l(&more_turns) > mohan_current_sheet_l(&base));

        let mut bigger = base;
        bigger.d_in = 80.0e-6;
        assert!(mohan_current_sheet_l(&bigger) > mohan_current_sheet_l(&base));
    }
}
