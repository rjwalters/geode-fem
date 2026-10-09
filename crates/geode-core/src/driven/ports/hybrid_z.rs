//! **Line-impedance accuracy estimate** of hybrid wave ports (Phase 5, #807
//! review).
//!
//! The per-mode `β` estimate ([`crate::analytic::port_mode_accuracy`]) is the
//! wrong proxy for a line impedance. `ε_eff = C/C₀` is a ratio, so most of the
//! strip-edge singular error cancels in `β`. `Z = 1/(c√(C·C₀))` keeps all of
//! it. On the cookbook microstrip the `β` estimate is `1.8e-3` while `Z_PI`
//! is about 4 % low from the mesh alone. And the error converges in `h` at
//! the singular rate, not `O(h²)`: measured about 0.9 in the strip-edge cell
//! size for a zero-thickness strip (an `r^{−1/2}` edge field gives an
//! `O(h)` energy, hence `C`, error on a quasi-uniform mesh).
//!
//! # Estimator
//!
//! The line quantities are re-evaluated on the same uniformly refined
//! (`h/2`) face the `β` estimate already solves. The refined conductor of a
//! coarse conductor is its coarse nodes plus the midpoints of its PEC edges.
//! Its voltage path is the coarse path through the edge midpoints. With the
//! matched refined mode (the `β` estimator's nested-prolongation match):
//!
//! ```text
//!   est = |Z_h − Z_{h/2}| / |Z_{h/2}| / (1 − 2^{−p})
//! ```
//!
//! per impedance (`Z_PI`, `Z_PV`, `Z_VI`). The rate `p` is **observed** from
//! a third level `h/4` at the first frequency
//! (`log₂(|Z_h − Z_{h/2}| / |Z_{h/2} − Z_{h/4}|)`), the `h/4` solve the `β`
//! rate already runs. It is clamped to `[0.5, 2]`: P1/Whitney energies
//! converge at most `O(h²)`, so a larger observed ratio is pre-asymptotic
//! noise that would understate the error. When the rate cannot be observed
//! (rate observation off, the `h/4` solve failed, or a difference at
//! round-off), the conservative singular rate [`SINGULAR_IMPEDANCE_RATE`]
//! (`p = 1`) is used. Assuming `p = 2` would understate the cookbook error
//! about 4–8×.
//!
//! The estimate is of **discretization** error only. A shielded line's
//! impedance depends on the shield. A small box lowers `Z₀` (about −3.3 %
//! for an `8h × 5h` box at `w/h = 1.9`, `ε_r = 4.4`). That is physics, so
//! no refinement removes it and this estimate does not flag it.

use std::collections::HashMap;

use faer::c64;
use faer::sparse::{SparseColMat, SparseColMatRef};

use super::hybrid::{
    HybridComplexLineReport, HybridLineReport, HybridPortFace, PortAccuracyOpts, PortWarning,
    PortWarningKind,
};
use crate::analytic::lossy_port_modes::{LossyHybridMode, dot_u, lossy_pairing_vector, rmatvec};
use crate::analytic::port_modes::{
    HybridBlocks, HybridPortMode, assemble_hybrid_blocks, discrete_gradient, sparse_matvec,
};
use crate::analytic::waveguide::{TriMesh, tri_p1_local};

const ZERO: c64 = c64 { re: 0.0, im: 0.0 };

/// Default [`super::PortAccuracyOpts::impedance_threshold`]: 1 % relative
/// line-impedance error.
///
/// A Touchstone file renormalized to a line impedance off by `δ` has a
/// spurious reflection `|Γ| ≈ δ/2` on a matched line. At 1 % that is
/// `|Γ| ≈ 0.005` (−46 dB), below a typical VNA calibration floor (−40 dB).
/// The `β` threshold (0.5 %) is tighter, but `β` converges at `O(h²)` and
/// `Z` only at the singular rate (about `O(h)` at a strip edge), so the
/// same bar on `Z` would ask for a much finer face than a design needs.
pub const DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD: f64 = 1e-2;

/// Rate used when the impedance convergence rate cannot be observed: the
/// singular strip-edge rate `p = 1` (conservative: a lower rate gives a
/// larger estimate). Module docs.
pub const SINGULAR_IMPEDANCE_RATE: f64 = 1.0;

/// Largest observed impedance rate accepted (module docs).
pub const MAX_IMPEDANCE_RATE: f64 = 2.0;

/// Smallest observed impedance rate accepted (module docs).
pub const MIN_IMPEDANCE_RATE: f64 = 0.5;

/// A line-impedance definition of a hybrid channel ([`HybridLineReport`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineImpedance {
    /// `Z_PI = 2P / Σ|I|²` (contour-independent; the default).
    #[default]
    PowerCurrent,
    /// `Z_PV = Σ|V|² / 2P` (path-dependent).
    PowerVoltage,
    /// `Z_VI = √(Z_PI · Z_PV)` (path-dependent).
    VoltageCurrent,
}

impl LineImpedance {
    /// Short label (`Z_PI`, `Z_PV`, `Z_VI`).
    pub fn label(self) -> &'static str {
        match self {
            Self::PowerCurrent => "Z_PI",
            Self::PowerVoltage => "Z_PV",
            Self::VoltageCurrent => "Z_VI",
        }
    }
}

/// Accuracy estimate of one line impedance of one channel (module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpedanceEstimate {
    /// Coarse (reported) impedance; real faces have `im = 0`.
    pub z: c64,
    /// The same impedance on the refined (`h/2`) face.
    pub z_refined: c64,
    /// `|Z_h − Z_{h/2}| / |Z_{h/2}|`.
    pub rel_change: f64,
    /// Rate `p` of the Richardson factor.
    pub rate: f64,
    /// `true` when [`Self::rate`] was observed from `h, h/2, h/4`;
    /// `false`: the conservative [`SINGULAR_IMPEDANCE_RATE`].
    pub rate_observed: bool,
    /// Estimated relative error of the coarse impedance:
    /// `rel_change / (1 − 2^{−p})`.
    pub estimate: f64,
}

impl ImpedanceEstimate {
    fn new(z: c64, z_refined: c64, rate: Option<f64>) -> Self {
        let rel_change = (z - z_refined).norm() / z_refined.norm();
        let (rate, rate_observed) = match rate {
            Some(p) => (p, true),
            None => (SINGULAR_IMPEDANCE_RATE, false),
        };
        Self {
            z,
            z_refined,
            rel_change,
            rate,
            rate_observed,
            estimate: rel_change / (1.0 - 2f64.powf(-rate)),
        }
    }

    /// The uniform refinement factor `(estimate / threshold)^{1/p}` (≥ 1)
    /// that brings the estimate down to `threshold`: every face cell,
    /// in particular the conductor-edge cells, must shrink by it.
    pub fn refine_factor(&self, threshold: f64) -> f64 {
        if self.estimate <= threshold || self.estimate <= 0.0 {
            return 1.0;
        }
        (self.estimate / threshold).powf(1.0 / self.rate)
    }
}

/// Per-channel line-impedance accuracy of a hybrid port (each impedance the
/// channel has; #807 review).
///
/// The `β` estimate is the wrong proxy for a line impedance: `ε_eff = C/C₀`
/// is a ratio, so the strip-edge singular error largely cancels in `β`, while
/// `Z = 1/(c√(C·C₀))` keeps it, and converges only at the singular rate
/// (about `O(h)` in the edge cell size). So the line quantities are
/// re-evaluated on the `h/2` face the `β` estimate solves (refined conductor:
/// the coarse conductor nodes plus the midpoints of its PEC edges; voltage
/// path through the edge midpoints) on the matched refined mode, and
///
/// ```text
///   est = |Z_h − Z_{h/2}| / |Z_{h/2}| / (1 − 2^{−p}),
/// ```
///
/// with `p` observed per channel and impedance over `h, h/2, h/4` at the
/// first frequency, clamped to [[`MIN_IMPEDANCE_RATE`],
/// [`MAX_IMPEDANCE_RATE`]], or [`SINGULAR_IMPEDANCE_RATE`] when it cannot be
/// observed. A `p = 2` assumption would understate the error several-fold.
///
/// Discretization error only: a shielded line's impedance also depends on
/// the shield (a small box lowers `Z₀`; physics), which this does not flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpedanceAccuracy {
    /// `Z_PI`.
    pub z_pi: ImpedanceEstimate,
    /// `Z_PV` (`None` without a voltage path).
    pub z_pv: Option<ImpedanceEstimate>,
    /// `Z_VI` (`None` without a voltage path).
    pub z_vi: Option<ImpedanceEstimate>,
}

impl ImpedanceAccuracy {
    /// The estimate of impedance `which`.
    pub fn get(&self, which: LineImpedance) -> Option<&ImpedanceEstimate> {
        match which {
            LineImpedance::PowerCurrent => Some(&self.z_pi),
            LineImpedance::PowerVoltage => self.z_pv.as_ref(),
            LineImpedance::VoltageCurrent => self.z_vi.as_ref(),
        }
    }
}

/// The three impedances of a line report as complex numbers
/// (`[Z_PI, Z_PV, Z_VI]`).
pub(super) type LineZ = [Option<c64>; 3];

pub(super) fn line_z(l: &HybridLineReport) -> LineZ {
    let r = |x: f64| c64::new(x, 0.0);
    [Some(r(l.z_pi)), l.z_pv.map(r), l.z_vi.map(r)]
}

pub(super) fn line_z_c(l: &HybridComplexLineReport) -> LineZ {
    [Some(l.z_pi), l.z_pv, l.z_vi]
}

/// Observed impedance rate from three nested levels (module docs); `None`
/// when a difference is at round-off.
pub(super) fn observed_impedance_rate(z_h: c64, z_h2: c64, z_h4: c64) -> Option<f64> {
    let d1 = (z_h - z_h2).norm();
    let d2 = (z_h2 - z_h4).norm();
    let floor = 1e-12 * z_h4.norm().max(1e-300);
    if d1 <= floor || d2 <= floor {
        return None;
    }
    Some(
        (d1 / d2)
            .log2()
            .clamp(MIN_IMPEDANCE_RATE, MAX_IMPEDANCE_RATE),
    )
}

/// Per-quantity observed rates of one channel from its three-level line
/// impedances.
pub(super) fn observed_rates(h: &LineZ, h2: &LineZ, h4: &LineZ) -> [Option<f64>; 3] {
    std::array::from_fn(|q| match (h[q], h2[q], h4[q]) {
        (Some(a), Some(b), Some(c)) => observed_impedance_rate(a, b, c),
        _ => None,
    })
}

/// The [`ImpedanceAccuracy`] of a channel from its coarse and refined line
/// impedances and the per-quantity rates; `None` if `Z_PI` is missing on
/// either level.
pub(super) fn impedance_accuracy(
    coarse: &LineZ,
    refined: &LineZ,
    rates: &[Option<f64>; 3],
) -> Option<ImpedanceAccuracy> {
    let est = |q: usize| match (coarse[q], refined[q]) {
        (Some(a), Some(b)) if b.norm() > 0.0 && a.is_finite() && b.is_finite() => {
            Some(ImpedanceEstimate::new(a, b, rates[q]))
        }
        _ => None,
    };
    Some(ImpedanceAccuracy {
        z_pi: est(0)?,
        z_pv: est(1),
        z_vi: est(2),
    })
}

/// Conductor node masks and voltage node paths of one refinement level.
#[derive(Debug, Clone)]
pub(super) struct LineLevel {
    conductors: Vec<Vec<bool>>,
    paths: Vec<Vec<u32>>,
}

impl LineLevel {
    /// The coarse level of `face`.
    pub(super) fn of_face(face: &HybridPortFace) -> Self {
        Self {
            conductors: face.conductors.iter().map(|c| c.nodes.clone()).collect(),
            paths: face
                .conductors
                .iter()
                .map(|c| c.voltage_path.clone())
                .collect(),
        }
    }

    /// The level on the uniform refinement of `coarse` (node `n₀ + e` is the
    /// midpoint of coarse edge `e` of `coarse.edges()`, the
    /// [`crate::analytic::port_mode_accuracy::UniformRefinement`] layout):
    /// a midpoint is on a conductor iff its edge is PEC
    /// (`interior_edge_mask[e] == false`) with both ends on it; a path runs
    /// through the midpoints of its edges.
    pub(super) fn refine(&self, coarse: &TriMesh, interior_edge_mask: &[bool]) -> Self {
        let edges = coarse.edges();
        let n0 = coarse.n_nodes() as u32;
        let idx: HashMap<(u32, u32), u32> = edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i as u32))
            .collect();
        let conductors = self
            .conductors
            .iter()
            .map(|c| {
                let mut f = c.clone();
                f.extend(
                    edges
                        .iter()
                        .zip(interior_edge_mask)
                        .map(|(e, &free)| !free && c[e[0] as usize] && c[e[1] as usize]),
                );
                f
            })
            .collect();
        let paths = self
            .paths
            .iter()
            .map(|p| {
                let mut out = Vec::with_capacity(2 * p.len());
                for w in p.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    out.push(a);
                    out.push(n0 + idx[&(a.min(b), a.max(b))]);
                }
                out.extend(p.last());
                out
            })
            .collect();
        Self { conductors, paths }
    }
}

/// The line-quantity operators of one face level.
pub(super) struct LineGeom {
    mesh: TriMesh,
    blocks: HybridBlocks,
    d: SparseColMat<usize, f64>,
    conductors: Vec<Vec<bool>>,
    paths: Vec<Vec<(usize, f64)>>,
}

impl LineGeom {
    /// The operators of `level` on `mesh` with real per-triangle `eps_r`
    /// (`Re ε` on a lossy face; the lossy path applies the full complex
    /// `ε` itself). `None` if the blocks do not assemble.
    pub(super) fn new(mesh: &TriMesh, eps_r: &[f64], level: &LineLevel) -> Option<Self> {
        let blocks = assemble_hybrid_blocks(mesh, eps_r).ok()?;
        let idx: HashMap<(u32, u32), usize> = mesh
            .edges()
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let paths = level
            .paths
            .iter()
            .map(|p| signed_path(p, &idx))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            mesh: mesh.clone(),
            d: discrete_gradient(mesh),
            blocks,
            conductors: level.conductors.clone(),
            paths,
        })
    }

    /// [`line_real`] on this level.
    pub(super) fn line(&self, m: &HybridPortMode, k0: f64) -> Option<HybridLineReport> {
        line_real(&self.blocks, &self.d, &self.conductors, &self.paths, m, k0)
    }

    /// [`line_complex`] on this level with per-triangle complex `eps`.
    pub(super) fn line_lossy(
        &self,
        m: &LossyHybridMode,
        k0: f64,
        eps: &[c64],
    ) -> Option<HybridComplexLineReport> {
        line_complex(
            &self.blocks,
            &self.d,
            &self.mesh,
            &self.conductors,
            &self.paths,
            m,
            k0,
            eps,
        )
    }
}

/// A node path as signed edges (`+1` along the edge's `lo → hi`).
pub(super) fn signed_path(
    p: &[u32],
    idx: &HashMap<(u32, u32), usize>,
) -> Option<Vec<(usize, f64)>> {
    p.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let e = *idx.get(&(a.min(b), a.max(b)))?;
            Some((e, if a < b { 1.0 } else { -1.0 }))
        })
        .collect()
}

/// Unit round-off `u = 2⁻⁵³` of `f64` (Higham's `u`; `f64::EPSILON / 2`).
const UNIT_ROUNDOFF: f64 = f64::EPSILON / 2.0;

/// Higham's `γ_n = n·u / (1 − n·u)`: the relative error bound of a chain of
/// `n` floating-point roundings (`+∞` once `n·u ≥ 1`).
fn gamma(n: usize) -> f64 {
    let nu = n as f64 * UNIT_ROUNDOFF;
    if nu >= 1.0 {
        f64::INFINITY
    } else {
        nu / (1.0 - nu)
    }
}

/// Per-conductor **round-off bound** of the discrete-Ampère sums
/// `q_c = Σ_{k∈c} (Gᵀẽ_t + Sẽ_z − k₀²T_εẽ_z)_k` (issue #953), from the
/// moduli of the mode's DOFs (`e_t_abs`, `e_z_abs`) and the displacement
/// term's per-node `(Σ |T_ε,kl|·|ẽ_z,l|, number of products)` (`t_abs`; the
/// lossy path assembles its complex `T_ε` per triangle, so the caller
/// supplies it).
///
/// `q_c` is a sum of `N_c` products (`N_c` = the stored entries of the
/// conductor's `G` columns and `S`, `T_ε` rows); `A_c` is the sum of the
/// products' moduli. However the sum is ordered, every product passes
/// through at most `N_c − 1` additions and `rounds` multiplicative
/// roundings, so the classic a-priori bound (Higham, *Accuracy and
/// Stability of Numerical Algorithms*, Thm 3.1 / eq. 3.5) is
/// `|fl(q_c) − q_c| ≤ γ_{N_c+rounds} · A_c`. On the real path `rounds = 3`
/// (the matrix entry, `k₀·k₀`, and `k₀²` times the `T_ε` row). On the lossy
/// path the `T_ε` term `ẽ_z·(ε·T_loc)` is a complex × complex product,
/// which costs `√2·γ₂` (Higham Lemma 3.5) instead of `γ₁`, so `rounds = 4`.
/// The bound always carries the `√2`; a real × complex product and a
/// complex addition round componentwise and need no more.
///
/// So a computed `|q_c|` at or below this bound is **indistinguishable from
/// zero** in floating point: the conductor carries no net current. It is
/// scale-free (it moves with the mode's normalization, `k₀` and the mesh
/// unit) and has no tuned constant. A conductor carrying a genuinely small
/// current (a mode near its cutoff) has `|q_c| / A_c` set by the physics,
/// many orders above `N_c·u`, and is not caught.
///
/// The bound covers the evaluation only, treating the mode and the blocks
/// as given. On the coax guide of #953 the TE₁₁ sums sit at 0.005–0.15 of
/// it and the TEM sum 12 orders above. A mode whose currents cancel only to
/// the eigensolver tolerance, not to round-off, would fall above the bound
/// and keep the numeric `Z_PI` and its accuracy warning, as before #953.
/// The test fails toward the old reporting, never toward hiding a line
/// impedance.
fn ampere_roundoff_bound(
    blocks: &HybridBlocks,
    t_abs: (&[f64], &[usize]),
    conductors: &[Vec<bool>],
    e_t_abs: &[f64],
    e_z_abs: &[f64],
    k0: f64,
    rounds: usize,
) -> Vec<f64> {
    // Per node: Σ|products| and the number of products.
    let (mut a, mut cnt) = abs_matvec(blocks.s.as_ref(), e_z_abs);
    let g = blocks.g.as_ref();
    let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
    for k in 0..a.len() {
        for p in cp[k]..cp[k + 1] {
            a[k] += v[p].abs() * e_t_abs[ri[p]];
        }
        cnt[k] += cp[k + 1] - cp[k] + t_abs.1[k];
        a[k] += k0 * k0 * t_abs.0[k];
    }
    conductors
        .iter()
        .map(|c| {
            let (sum, terms) = (0..c.len())
                .filter(|&k| c[k])
                .fold((0.0, 0usize), |(s, t), k| (s + a[k], t + cnt[k]));
            std::f64::consts::SQRT_2 * gamma(terms + rounds) * sum
        })
        .collect()
}

/// `(|A|·x, products per row)` of a sparse matrix and a non-negative `x`.
fn abs_matvec(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> (Vec<f64>, Vec<usize>) {
    let mut y = vec![0.0; a.nrows()];
    let mut cnt = vec![0usize; a.nrows()];
    let (cp, ri, v) = (a.col_ptr(), a.row_idx(), a.val());
    for (j, &xj) in x.iter().enumerate() {
        for p in cp[j]..cp[j + 1] {
            y[ri[p]] += v[p].abs() * xj;
            cnt[ri[p]] += 1;
        }
    }
    (y, cnt)
}

/// `true` when **every** conductor's net current is at round-off
/// ([`ampere_roundoff_bound`]): the channel carries no net conductor current
/// (a TE / TM waveguide mode of the face, such as a coax TE₁₁), so a
/// current-based line impedance (`Z_PI`, `Z_VI`) is undefined. Tested per
/// conductor, not on `Σ I_c`: an odd mode of a coupled pair has opposite but
/// non-zero currents. `false` without conductors.
fn no_net_current(q_abs: &[f64], bound: &[f64]) -> bool {
    !q_abs.is_empty() && q_abs.iter().zip(bound).all(|(q, b)| q <= b)
}

/// Signed discrete-Ampère conductor currents of a real mode at `k0`:
/// `I_c = −(1/k₀η₀) Σ_{k∈c} (Gᵀẽ_t + Sẽ_z − k₀²T_εẽ_z)_k`.
pub(super) fn currents_real(
    blocks: &HybridBlocks,
    conductors: &[Vec<bool>],
    m: &HybridPortMode,
    k0: f64,
) -> Vec<f64> {
    if conductors.is_empty() {
        return Vec::new();
    }
    let g = blocks.g.as_ref();
    let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
    let mut zrow = vec![0.0; g.ncols()];
    for (j, o) in zrow.iter_mut().enumerate() {
        for p in cp[j]..cp[j + 1] {
            *o += v[p] * m.e_t[ri[p]];
        }
    }
    let s_ez = sparse_matvec(blocks.s.as_ref(), &m.e_z);
    let t_ez = sparse_matvec(blocks.t_eps.as_ref(), &m.e_z);
    let eta = crate::constants::ETA_0_OHM;
    conductors
        .iter()
        .map(|c| {
            let q: f64 = (0..c.len())
                .filter(|&k| c[k])
                .map(|k| zrow[k] + s_ez[k] - k0 * k0 * t_ez[k])
                .sum();
            -q / (k0 * eta)
        })
        .collect()
}

/// [`HybridLineReport`] of a propagating real mode (`None` without
/// conductors or for `β² ≤ 0`).
pub(super) fn line_real(
    blocks: &HybridBlocks,
    d: &SparseColMat<usize, f64>,
    conductors: &[Vec<bool>],
    paths: &[Vec<(usize, f64)>],
    m: &HybridPortMode,
    k0: f64,
) -> Option<HybridLineReport> {
    if conductors.is_empty() || !m.is_propagating() {
        return None;
    }
    let beta = m.beta.re;
    let eta = crate::constants::ETA_0_OHM;
    let mut w = sparse_matvec(d.as_ref(), &m.e_z);
    for (wi, ti) in w.iter_mut().zip(&m.e_t) {
        *wi += ti;
    }
    let m1w = sparse_matvec(blocks.m1.as_ref(), &w);
    let xbx: f64 = m.e_t.iter().zip(&m1w).map(|(a, b)| a * b).sum();
    let power = xbx / (2.0 * k0 * eta * beta);
    let currents = currents_real(blocks, conductors, m, k0);
    let voltages: Vec<Option<f64>> = paths
        .iter()
        .map(|p| {
            (!p.is_empty()).then(|| p.iter().map(|&(e, sg)| sg * m.e_t[e]).sum::<f64>() / beta)
        })
        .collect();
    let e_t_abs: Vec<f64> = m.e_t.iter().map(|x| x.abs()).collect();
    let e_z_abs: Vec<f64> = m.e_z.iter().map(|x| x.abs()).collect();
    let (t_a, t_n) = abs_matvec(blocks.t_eps.as_ref(), &e_z_abs);
    let bound = ampere_roundoff_bound(blocks, (&t_a, &t_n), conductors, &e_t_abs, &e_z_abs, k0, 3);
    let q_abs: Vec<f64> = currents.iter().map(|i| i.abs() * (k0 * eta)).collect();
    let no_net_current = no_net_current(&q_abs, &bound);
    let i2: f64 = currents.iter().map(|i| i * i).sum();
    let z_pi = 2.0 * power / i2;
    let v2: Option<f64> = voltages.iter().map(|v| v.map(|v| v * v)).sum();
    let z_pv = v2.map(|v2| v2 / (2.0 * power));
    Some(HybridLineReport {
        power,
        currents,
        voltages,
        z_pi,
        z_pv,
        z_vi: z_pv.map(|z| (z * z_pi).sqrt()),
        no_net_current,
    })
}

/// [`HybridComplexLineReport`] of a propagating lossy mode on per-triangle
/// complex `eps` at `k0` (`None` without conductors or for `Re β² ≤ 0`).
/// The displacement term of the currents uses the full complex `T_ε` of
/// `eps` (so a dispersive face's `Re ε(ω)` is honoured).
#[allow(clippy::too_many_arguments)]
pub(crate) fn line_complex(
    blocks: &HybridBlocks,
    d: &SparseColMat<usize, f64>,
    mesh: &TriMesh,
    conductors: &[Vec<bool>],
    paths: &[Vec<(usize, f64)>],
    m: &LossyHybridMode,
    k0: f64,
    eps: &[c64],
) -> Option<HybridComplexLineReport> {
    if conductors.is_empty() || !m.is_propagating() {
        return None;
    }
    let eta = crate::constants::ETA_0_OHM;
    // q = Gᵀẽ_t + Sẽ_z − k₀² T_ε ẽ_z (complex), I = −q/(k₀η₀).
    let g = blocks.g.as_ref();
    let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
    let mut q = vec![ZERO; g.ncols()];
    for (j, o) in q.iter_mut().enumerate() {
        for p in cp[j]..cp[j + 1] {
            *o += m.e_t[ri[p]] * v[p];
        }
    }
    let mut s_ez = vec![ZERO; q.len()];
    rmatvec(blocks.s.as_ref(), &m.e_z, &mut s_ez);
    let mut t_ez = vec![ZERO; q.len()];
    // Its round-off scale (`|ε|·|T_loc|·|ẽ_z|` per node, and the count).
    let mut t_a = vec![0.0; q.len()];
    let mut t_n = vec![0usize; q.len()];
    for (tri, e) in mesh.tris.iter().zip(eps) {
        let coords = tri.map(|n| mesh.nodes[n as usize]);
        let (_s, t_loc, _) = tri_p1_local(&coords);
        for p in 0..3 {
            for r in 0..3 {
                let ez = m.e_z[tri[r] as usize];
                t_ez[tri[p] as usize] += ez * (*e * t_loc[p][r]);
                t_a[tri[p] as usize] += e.norm() * t_loc[p][r].abs() * ez.norm();
                t_n[tri[p] as usize] += 1;
            }
        }
    }
    for ((o, s), t) in q.iter_mut().zip(&s_ez).zip(&t_ez) {
        *o += *s - *t * (k0 * k0);
    }
    let currents: Vec<c64> = conductors
        .iter()
        .map(|c| {
            let s = (0..c.len())
                .filter(|&k| c[k])
                .fold(ZERO, |acc, k| acc + q[k]);
            -s / (k0 * eta)
        })
        .collect();
    let w = lossy_pairing_vector(d, m);
    let mut m1w = vec![ZERO; w.len()];
    rmatvec(blocks.m1.as_ref(), &w, &mut m1w);
    let xbx = dot_u(&m.e_t, &m1w);
    let power = xbx / (c64::new(2.0 * k0 * eta, 0.0) * m.beta);
    let voltages: Vec<Option<c64>> = paths
        .iter()
        .map(|p| {
            (!p.is_empty())
                .then(|| p.iter().fold(ZERO, |acc, &(e, sg)| acc + m.e_t[e] * sg) / m.beta)
        })
        .collect();
    let e_t_abs: Vec<f64> = m.e_t.iter().map(|x| x.norm()).collect();
    let e_z_abs: Vec<f64> = m.e_z.iter().map(|x| x.norm()).collect();
    let bound = ampere_roundoff_bound(blocks, (&t_a, &t_n), conductors, &e_t_abs, &e_z_abs, k0, 4);
    let q_abs: Vec<f64> = currents.iter().map(|i| i.norm() * (k0 * eta)).collect();
    let no_net_current = no_net_current(&q_abs, &bound);
    let i2: c64 = currents.iter().map(|i| i * i).sum();
    let z_pi = c64::new(2.0, 0.0) * power / i2;
    let v2: Option<c64> = voltages.iter().map(|v| v.map(|v| v * v)).sum();
    let z_pv = v2.map(|v2| v2 / (c64::new(2.0, 0.0) * power));
    Some(HybridComplexLineReport {
        power,
        currents,
        voltages,
        z_pi,
        z_pv,
        z_vi: z_pv.map(|z| (z * z_pi).sqrt()),
        no_net_current,
    })
}

/// The impedance estimate of one channel at one frequency.
pub(super) enum ZOutcome {
    /// No line impedance (no conductor, not propagating, or no net
    /// conductor current, #953).
    NotApplicable,
    /// The estimate.
    Estimate(ImpedanceAccuracy),
    /// The channel has a line impedance but no estimate, and why.
    Unavailable(String),
}

impl ZOutcome {
    /// The estimate, if any.
    pub(super) fn accuracy(&self) -> Option<ImpedanceAccuracy> {
        match self {
            Self::Estimate(a) => Some(*a),
            _ => None,
        }
    }
}

/// Pending impedance warnings of one port over a sweep: the worst
/// above-threshold estimate, the first unavailable one and the first
/// no-net-current one (#953) per channel.
pub(super) struct ZWarnings {
    above: Vec<Option<PortWarningKind>>,
    unavailable: Vec<Option<PortWarningKind>>,
    no_current: Vec<Option<PortWarningKind>>,
}

impl ZWarnings {
    pub(super) fn new(k: usize) -> Self {
        Self {
            above: vec![None; k],
            unavailable: vec![None; k],
            no_current: vec![None; k],
        }
    }

    /// Record that channel `c`'s line at `omega` carries no net conductor
    /// current ([`HybridLineReport::no_net_current`]; issue #953), with or
    /// without the accuracy estimate: the first frequency is kept.
    pub(super) fn record_no_current(&mut self, c: usize, omega: f64) {
        if self.no_current[c].is_none() {
            self.no_current[c] = Some(PortWarningKind::NoNetConductorCurrent { channel: c, omega });
        }
    }

    /// Record channel `c`'s outcome at `omega` (`h`, `h_min`: the face's
    /// largest and smallest edge).
    pub(super) fn record(
        &mut self,
        c: usize,
        omega: f64,
        outcome: &ZOutcome,
        acc: &PortAccuracyOpts,
        h: f64,
        h_min: f64,
    ) {
        match outcome {
            ZOutcome::NotApplicable => {}
            ZOutcome::Unavailable(reason) => {
                if self.unavailable[c].is_none() {
                    self.unavailable[c] = Some(PortWarningKind::ImpedanceAccuracyUnavailable {
                        channel: c,
                        omega,
                        reason: reason.clone(),
                    });
                }
            }
            ZOutcome::Estimate(a) => {
                let Some(thr) = acc.impedance_threshold else {
                    return;
                };
                let Some(e) = a.get(acc.impedance) else {
                    if self.unavailable[c].is_none() {
                        self.unavailable[c] = Some(PortWarningKind::ImpedanceAccuracyUnavailable {
                            channel: c,
                            omega,
                            reason: format!("{} needs a voltage path", acc.impedance.label()),
                        });
                    }
                    return;
                };
                if e.estimate <= thr {
                    return;
                }
                let worse = match &self.above[c] {
                    Some(PortWarningKind::ImpedanceAccuracyAboveThreshold { estimate, .. }) => {
                        e.estimate > *estimate
                    }
                    _ => true,
                };
                if worse {
                    let f = e.refine_factor(thr);
                    self.above[c] = Some(PortWarningKind::ImpedanceAccuracyAboveThreshold {
                        channel: c,
                        omega,
                        impedance: acc.impedance,
                        z: e.z,
                        estimate: e.estimate,
                        threshold: thr,
                        rate: e.rate,
                        rate_observed: e.rate_observed,
                        refine_factor: f,
                        h,
                        h_required: h / f,
                        h_min,
                        h_min_required: h_min / f,
                    });
                }
            }
        }
    }

    /// The port's impedance warnings.
    pub(super) fn into_warnings(self, p_idx: usize) -> Vec<PortWarning> {
        let mut out = Vec::new();
        for kind in self.above.into_iter().flatten() {
            let PortWarningKind::ImpedanceAccuracyAboveThreshold {
                channel,
                omega,
                impedance,
                z,
                estimate,
                threshold,
                rate,
                rate_observed,
                refine_factor,
                h,
                h_required,
                h_min,
                h_min_required,
            } = &kind
            else {
                unreachable!()
            };
            let z_s = if z.im == 0.0 {
                format!("{:.3} Ω", z.re)
            } else {
                format!("{:.3}{:+.3}j Ω", z.re, z.im)
            };
            let rate_s = if *rate_observed {
                format!("observed convergence rate p = {rate:.2}")
            } else {
                format!("conservative singular rate p = {rate:.2} (rate not observed)")
            };
            out.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: estimated {} error {:.2} % at \
                     ω = {omega} exceeds {:.2} % ({} = {z_s}; {rate_s}). Line impedances \
                     converge slowly (the conductor-edge field is singular): refine the port \
                     face about {refine_factor:.1}×, grading toward the conductor edges — edge \
                     cells ≤ {h_min_required:.4e} (now {h_min:.4e}), largest cells ≤ \
                     {h_required:.4e} (now {h:.4e}). This estimate covers discretization only: \
                     a shielded line's impedance also depends on the shield box, which no \
                     refinement changes",
                    impedance.label(),
                    100.0 * estimate,
                    100.0 * threshold,
                    impedance.label(),
                ),
                kind,
            });
        }
        for kind in self.unavailable.into_iter().flatten() {
            let PortWarningKind::ImpedanceAccuracyUnavailable {
                channel,
                omega,
                reason,
            } = &kind
            else {
                unreachable!()
            };
            out.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: line-impedance accuracy \
                     estimate unavailable at ω = {omega} ({reason}); the line impedance is \
                     reported without an error estimate"
                ),
                kind,
            });
        }
        for kind in self.no_current.into_iter().flatten() {
            let PortWarningKind::NoNetConductorCurrent { channel, omega } = &kind else {
                unreachable!()
            };
            out.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: the mode carries no net \
                     conductor current (from ω = {omega}: every conductor's current cancels to \
                     floating-point round-off) — a TE/TM waveguide mode of the port face, not a \
                     line mode — so Z_PI and Z_VI are undefined and no line impedance or \
                     line-impedance accuracy is reported for it. This is not a mesh problem; \
                     its modal S is still valid"
                ),
                kind,
            });
        }
        out
    }
}

/// Smallest edge length of a 2-D mesh (the conductor-edge cell size of a
/// graded face).
pub(super) fn min_edge(mesh: &TriMesh) -> f64 {
    mesh.edges()
        .iter()
        .map(|e| {
            let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
            (q[0] - p[0]).hypot(q[1] - p[1])
        })
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_uses_the_rate_and_falls_back_to_the_singular_rate() {
        let z = c64::new(46.0, 0.0);
        let z2 = c64::new(48.0, 0.0);
        let e = ImpedanceEstimate::new(z, z2, None);
        assert!(!e.rate_observed);
        assert_eq!(e.rate, SINGULAR_IMPEDANCE_RATE);
        // p = 1: twice the h → h/2 change.
        assert!((e.estimate - 2.0 * 2.0 / 48.0).abs() < 1e-14);
        let e2 = ImpedanceEstimate::new(z, z2, Some(2.0));
        assert!((e2.estimate - (2.0 / 48.0) / 0.75).abs() < 1e-14);
        // Refine factor: (est/τ)^{1/p}.
        assert!((e.refine_factor(e.estimate / 4.0) - 4.0).abs() < 1e-12);
        assert_eq!(e.refine_factor(1.0), 1.0);
    }

    #[test]
    fn gamma_is_the_higham_bound_and_saturates() {
        assert_eq!(gamma(0), 0.0);
        let u = f64::EPSILON / 2.0;
        assert!((gamma(100) / (100.0 * u) - 1.0).abs() < 1e-12);
        assert_eq!(gamma(usize::MAX), f64::INFINITY);
    }

    /// #953: the channel has no net current only when **every** conductor
    /// is at or below its round-off bound; an odd coupled pair (opposite,
    /// non-zero currents, which cancel in `Σ I`) and a pair with one
    /// carrying conductor are kept; no conductors is never flagged.
    #[test]
    fn no_net_current_needs_every_conductor_at_round_off() {
        assert!(no_net_current(&[1e-14], &[1e-13]));
        assert!(no_net_current(&[1e-14, 0.0], &[1e-13, 1e-13]));
        assert!(!no_net_current(&[6.0], &[1e-12]));
        assert!(!no_net_current(&[1.0, 1.0], &[1e-13, 1e-13]));
        assert!(!no_net_current(&[1e-14, 1e-3], &[1e-13, 1e-13]));
        assert!(!no_net_current(&[], &[]));
    }

    #[test]
    fn observed_rate_is_clamped_and_round_off_is_unobserved() {
        let r = |a: f64, b: f64, c: f64| {
            observed_impedance_rate(c64::new(a, 0.0), c64::new(b, 0.0), c64::new(c, 0.0))
        };
        // Differences 2, 1: rate 1.
        assert!((r(46.0, 48.0, 49.0).unwrap() - 1.0).abs() < 1e-12);
        // Differences 2, 0.01: clamped to 2.
        assert_eq!(r(46.0, 48.0, 48.01), Some(MAX_IMPEDANCE_RATE));
        // Diverging: clamped to 0.5.
        assert_eq!(r(46.0, 48.0, 52.0), Some(MIN_IMPEDANCE_RATE));
        assert_eq!(r(48.0, 48.0, 49.0), None);
    }
}
