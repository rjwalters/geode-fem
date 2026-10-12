//! Transmon qubit–resonator coupling `g` from an avoided-crossing `L_J`
//! sweep, and the bare-qubit `C_Σ` anchor it implies (issue #1078).
//!
//! The DeviceLayout anchor ([`TransmonAnchor::devicelayout`]) reads the
//! published 4.14 GHz through the linear `L_J ∥ (C_Σ + C_J)` model. That
//! 4.14 GHz is the lower of two coupled modes (`[4.14, 5.591]` GHz), while
//! geode's `C_Σ` holds the resonator at 0 V. This generator measures the
//! coupling and derives the bare-qubit target in five parts.
//!
//! 1. **Sweep.** On the committed 133k-tet fixture, with the A2 port-voltage
//!    shunt ([`JunctionPortModel::PortVoltage`], #1044) and the plain bulk
//!    projector, it solves the three lowest physical modes at each `L_J` of
//!    [`SWEEP_LJ_NH`]. The bare qubit crosses the 5.15 GHz λ/4 resonator near
//!    `L_J` = 6.6 nH. One `K`, `M` assembly is reused; each `L_J` is one
//!    factorization.
//! 2. **Fit.** Three models are fitted to the two hybridized branches, each
//!    with `f_q = a/√(L_J + L_s)`. `L_s` is a series loop inductance: the
//!    qubit mode keeps a fraction `L_s/(L_J + L_s)` of its inductive energy
//!    outside the junction, which the participations show directly.
//!    - the capacitively coupled two-node LC circuit in Maxwell form,
//!      `(f_q² − f²)(f_r² − f²) = k² f⁴`, fit parameters `a`, `f_r`, `k`,
//!      `L_s` (the primary model, see below);
//!    - the issue's rotating-wave two-mode formula with a constant `g`;
//!    - the Maxwell circuit with the resonator's 3λ/4 harmonic (the 15.46 GHz
//!      mode) as a third node, fitted to all three modes.
//!
//!    Residuals are reported per point, and the fits are repeated on a
//!    near-crossing window. The fit uses frequencies only; the junction
//!    participations are compared with the fitted circuit's eigenvectors.
//! 3. **Palace.** Committed Palace runs (local docker image `palace:cpu-1043`)
//!    at three `L_J` near the crossing on the same mesh, and an `L_J` scan on
//!    one uniform refinement (`h/2`), are read from
//!    `reference/fixtures/transmon_palace/results_coupling_1078/`. The `h`
//!    cells cross-check geode; the `h/2` cells measure `k` on a second
//!    resolution.
//! 4. **Static proxy.** A region-split electrode (the #1037 Phase C idea):
//!    the resonator centre trace, carved out of the ground conductor by a
//!    flood fill that stops at its shorted end, is its own conductor. The
//!    tensor-ε P1 Maxwell matrix on `h`, `h/2` and `h/4` gives `C_Σ`, the
//!    island-to-resonator `C_g`, the trace self-capacitance `C_rr` and the
//!    static coupling `k_s = C_g / √((C_Σ + C_J) C_rr/2)` (a λ/4 line's
//!    lumped equivalent is half its line capacitance).
//! 5. **Bare anchor.** `k` rises under refinement at every measured level,
//!    so the anchor uses an extrapolated converged `k` with a band (the rule
//!    is in `[anchor].k_rule`, and the generator asserts that the
//!    `quantum::transmon` constants follow it). The Maxwell-form inversion of
//!    DeviceLayout's dressed pair with `k` gives the resonator-grounded qubit
//!    frequency, which is the one `C_Σ` describes, and so the bare-qubit
//!    `C_Σ` target ([`TransmonAnchor::devicelayout_bare`]). The resonator's
//!    odd harmonics widen the band's lower end. The #1077 brackets are
//!    re-evaluated against it.
//!
//! # Why the Maxwell form
//!
//! geode's `C_Σ` is the island's Maxwell diagonal with the resonator at 0 V
//! (Koch's `C_B + C_g`). In the two-node circuit that diagonal gives
//! `f_q = 1/(2π √(L_J (C_Σ + C_J)))`, the frequency the qubit would have with
//! the resonator shorted. That is the Maxwell-form `f_q`. The Hamiltonian
//! (rotating-wave) bare frequency of the same circuit is `f_q / √(1 − k²)`,
//! higher by `k²/2` relative. At this anchor that difference is comparable to
//! the #1077 threshold, so the choice matters; both are reported.
//!
//! # Usage
//!
//! ```sh
//! cargo run -p geode-core --release --example transmon_coupling_g
//! ```
//!
//! Writes `benchmarks/transmon_coupling/results.toml` (override the
//! directory with `$TRANSMON_COUPLING_BENCH_DIR`). About 15 to 20 min of
//! eigen solves (about 3 GB peak each) and 80 s of electrostatics (8.6 GB
//! peak RSS at `h/4`) on 28 local cores. The Palace cells are not re-run;
//! `reference/fixtures/transmon_palace/results_coupling_1078/run.sh`
//! reproduces them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::electrostatic::{
    AmgPcgOptions, CapacitanceMatrix, Electrode, assemble_electrostatic_tensor,
    extract_capacitance_amg_pcg,
};
use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model;
use geode_core::eigen::transmon::{
    JunctionPortModel, LumpedReactiveShunt, ReactiveElementNatural, TransmonPencil,
    lambda_shift_for_frequency_hz,
};
use geode_core::mesh::red_refine::red_refine;
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::transmon::{JUNCTION_CAPACITANCE_F, JUNCTION_INDUCTANCE_H, sapphire_eps_lab};
use geode_core::mesh::{MetalRole, TetMesh, TransmonFixture, read_transmon_smoke_fixture};
use geode_core::quantum::transmon::{
    DEVICELAYOUT_COUPLING_K, DEVICELAYOUT_COUPLING_K_BAND, DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
    DEVICELAYOUT_RESONATOR_FREQUENCY_HZ, TransmonAnchor, coupled_lc_bare_pair_hz,
    coupled_lc_dressed_pair_hz,
};
use geode_core::testing::TestBackend;
use geode_util::fixture::BackendInfo;

type B = TestBackend;

/// Metres per mesh unit (micrometre mesh).
const M_PER_UNIT: f64 = 1e-6;

/// Junction inductances of the sweep (nH): a wide bracket for the model
/// check, dense around the crossing near 6.6 nH, the Palace `h` cells
/// (6.40, 6.60, 6.80) and 12.17 nH, where the dressed-frequency ratio is
/// DeviceLayout's 4.14/5.591.
const SWEEP_LJ_NH: [f64; 27] = [
    4.0, 5.0, 6.0, 6.2, 6.3, 6.4, 6.45, 6.5, 6.55, 6.6, 6.625, 6.65, 6.675, 6.7, 6.75, 6.8, 6.9,
    7.0, 7.25, 7.5, 8.0, 9.0, 10.0, 12.17, 14.86, 20.0, 30.0,
];

/// The near-crossing fit window (nH).
const WINDOW_LJ_NH: (f64, f64) = (6.2, 7.25);

/// Shift of every sweep solve (GHz) and modes requested.
const SIGMA_GHZ: f64 = 5.0;
const N_MODES: usize = 3;

/// Palace cells (committed under `PALACE_DIR`): name, mesh level (0 = `h`,
/// 1 = `h/2`) and `L_J` (nH).
const PALACE_DIR: &str = "reference/fixtures/transmon_palace/results_coupling_1078";
const PALACE_CELLS: [(&str, usize, f64); 3] = [
    ("h_L6p40", 0, 6.40),
    ("h_L6p60", 0, 6.60),
    ("h_L6p80", 0, 6.80),
];
const PALACE_H2_CELLS: &[(&str, f64)] = &[
    ("h2_L5p50", 5.50),
    ("h2_L7p20", 7.20),
    ("h2_L7p50", 7.50),
    ("h2_L7p65", 7.65),
    ("h2_L7p75", 7.75),
    ("h2_L7p90", 7.90),
    ("h2_L8p20", 8.20),
    ("h2_L10p00", 10.00),
    ("h2_L14p86", 14.86),
];
/// Linear-solver control: the `h` cell at 6.60 nH re-run with the AMS
/// solver the `h/2` cells use, against the committed SuperLU cell.
const PALACE_AMS_CONTROL: (&str, &str) = ("hams_L6p60", "h_L6p60");

/// The resonator's shorted end on the fixture (μm): the centre trace meets
/// the ground plane at `x = −200`, `y ∈ [2235, 2245]`; the CPW gaps are
/// `y ∈ [2229, 2235]` and `[2245, 2251]`.
const SHORT_X_UM: f64 = -200.0;
const SHORT_Y_UM: (f64, f64) = (2229.0, 2251.0);
/// A point inside the trace (the straight section along the feedline), as
/// a window for the flood-fill seed (μm).
const SEED_WINDOW_UM: ([f64; 2], [f64; 2]) = ([-150.0, 150.0], [2236.0, 2244.0]);
/// The claw-only electrode is the trace below this `y` (μm).
const CLAW_Y_UM: f64 = 720.0;

/// #1077 (#1075) certified brackets of the two re-anchored designs (fF,
/// unrounded family bounds of
/// `benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml`): (lower,
/// upper).
const BRACKET_1066_DESIGN_FF: (f64, f64) = (92.758828, 92.998561);
const BRACKET_CORRECTED_FF: (f64, f64) = (93.891607, 94.136104);

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Small dense linear algebra.
// ---------------------------------------------------------------------------

/// Eigenvalues and eigenvectors (columns) of a real symmetric matrix by
/// cyclic Jacobi rotations, eigenvalues ascending.
fn sym_eig(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut a: Vec<Vec<f64>> = a.to_vec();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for _ in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        let diag: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum();
        if off <= 1e-34 * diag {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q] == 0.0 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (rp, rq) = (a[p].clone(), a[q].clone());
                for k in 0..n {
                    a[p][k] = c * rp[k] - s * rq[k];
                    a[q][k] = s * rp[k] + c * rq[k];
                }
                for row in v.iter_mut() {
                    let (vkp, vkq) = (row[p], row[q]);
                    row[p] = c * vkp - s * vkq;
                    row[q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let vals = idx.iter().map(|&i| a[i][i]).collect();
    let vecs = idx
        .iter()
        .map(|&i| (0..n).map(|k| v[k][i]).collect())
        .collect();
    (vals, vecs)
}

/// Solve the dense SPD-ish system `a x = b` by Gaussian elimination with
/// partial pivoting.
fn solve_dense(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut m: Vec<Vec<f64>> = a
        .iter()
        .zip(b)
        .map(|(r, &bi)| {
            let mut r = r.clone();
            r.push(bi);
            r
        })
        .collect();
    for c in 0..n {
        let p = (c..n)
            .max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))
            .unwrap();
        m.swap(c, p);
        for r in c + 1..n {
            let f = m[r][c] / m[c][c];
            let pivot = m[c].clone();
            for (x, pv) in m[r].iter_mut().zip(&pivot).skip(c) {
                *x -= f * pv;
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| m[r][k] * x[k]).sum();
        x[r] = (m[r][n] - s) / m[r][r];
    }
    x
}

fn invert(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let cols: Vec<Vec<f64>> = (0..n)
        .map(|j| {
            let e: Vec<f64> = (0..n).map(|i| if i == j { 1.0 } else { 0.0 }).collect();
            solve_dense(a, &e)
        })
        .collect();
    (0..n)
        .map(|i| (0..n).map(|j| cols[j][i]).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// Coupled-mode models.
// ---------------------------------------------------------------------------

/// Normal modes of `n` capacitively coupled LC nodes in Maxwell form:
/// uncoupled (node-grounded) frequencies `f` (GHz) and normalized couplings
/// `k[i][j]` (the off-diagonal of `D^{-1/2} C D^{-1/2}` with a minus sign).
/// Returns the mode frequencies (GHz, ascending) and, per mode, each node's
/// inductive-energy fraction.
///
/// With `u_i = Φ_i / √L_i`, `L⁻¹ Φ = ω² C Φ` becomes
/// `Ω^{-1/2} (I − K) Ω^{-1/2} u = ω⁻² u`, `Ω = diag(ω_i²)`, and `u_i²` is
/// node `i`'s share of the inductive energy.
fn maxwell_modes(f: &[f64], k: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = f.len();
    let a: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let c = if i == j { 1.0 } else { -k[i][j] };
                    c / (f[i] * f[j])
                })
                .collect()
        })
        .collect();
    let (mu, vecs) = sym_eig(&a);
    // mu = 1/f² ascending -> frequencies descending; reverse.
    let mut out: Vec<(f64, Vec<f64>)> = mu
        .iter()
        .zip(vecs)
        .map(|(&m, v)| {
            let s: f64 = v.iter().map(|x| x * x).sum();
            (1.0 / m.sqrt(), v.iter().map(|x| x * x / s).collect())
        })
        .collect();
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    out.into_iter().unzip()
}

/// A sweep point: `L_J` (nH), the three lowest physical modes (GHz), their
/// junction participations, residuals and divergence ratios.
#[derive(Clone, Debug)]
struct Point {
    l_nh: f64,
    f: Vec<f64>,
    p: Vec<f64>,
    res: Vec<f64>,
    div: Vec<f64>,
    solve_s: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Model {
    /// Two-node Maxwell circuit: `a`, `f_r`, `k`.
    Maxwell2,
    /// Rotating-wave two-mode formula: `a`, `f_r`, `g`.
    Rwa2,
    /// Three-node Maxwell circuit (qubit, λ/4, 3λ/4): `a`, `f_r`, `k`, `f_3`, `k_3`.
    Maxwell3,
}

impl Model {
    fn name(self) -> &'static str {
        match self {
            Model::Maxwell2 => "maxwell2",
            Model::Rwa2 => "rwa2",
            Model::Maxwell3 => "maxwell3",
        }
    }
    fn params(self) -> &'static [&'static str] {
        match self {
            Model::Maxwell2 => &["a_ghz_sqrt_nh", "f_r_ghz", "k", "l_s_nh"],
            Model::Rwa2 => &["a_ghz_sqrt_nh", "f_r_ghz", "g_ghz", "l_s_nh"],
            Model::Maxwell3 => &["a_ghz_sqrt_nh", "f_r_ghz", "k", "l_s_nh", "f_3_ghz", "k_3"],
        }
    }
    fn n_branches(self) -> usize {
        match self {
            Model::Maxwell3 => 3,
            _ => 2,
        }
    }
    /// Predicted mode frequencies (GHz) at `l_nh`.
    fn predict(self, th: &[f64], l_nh: f64) -> Vec<f64> {
        let fq = th[0] / (l_nh + th[3]).sqrt();
        match self {
            Model::Maxwell2 => {
                let k = vec![vec![0.0, th[2]], vec![th[2], 0.0]];
                maxwell_modes(&[fq, th[1]], &k).0
            }
            Model::Rwa2 => {
                let (m, d) = (0.5 * (fq + th[1]), 0.5 * (fq - th[1]));
                let s = (d * d + th[2] * th[2]).sqrt();
                vec![m - s, m + s]
            }
            Model::Maxwell3 => {
                let k = vec![
                    vec![0.0, th[2], th[5]],
                    vec![th[2], 0.0, 0.0],
                    vec![th[5], 0.0, 0.0],
                ];
                maxwell_modes(&[fq, th[1], th[4]], &k).0
            }
        }
    }
}

/// A least-squares fit result.
#[derive(Clone, Debug)]
struct Fit {
    model: Model,
    window: &'static str,
    theta: Vec<f64>,
    stderr: Vec<f64>,
    rms_mhz: f64,
    max_abs_mhz: f64,
    n_data: usize,
    /// Per point, per branch: model − data (MHz).
    residuals_mhz: Vec<(f64, Vec<f64>)>,
}

fn residual_vec(model: Model, th: &[f64], pts: &[&Point]) -> Vec<f64> {
    let nb = model.n_branches();
    pts.iter()
        .flat_map(|p| {
            let m = model.predict(th, p.l_nh);
            (0..nb).map(move |b| m[b] - p.f[b]).collect::<Vec<_>>()
        })
        .collect()
}

/// Levenberg–Marquardt on the branch frequencies with a central-difference
/// Jacobian, over every parameter. The data are deterministic solves, so the
/// reported standard errors (`s² (JᵀJ)⁻¹`, `s² = SSR/(n − p)`) measure model
/// misfit, not noise.
fn fit(model: Model, window: &'static str, pts: &[&Point], th0: &[f64]) -> Fit {
    let free: Vec<usize> = (0..th0.len()).collect();
    fit_free(model, window, pts, th0, &free)
}

/// [`fit`] over the parameters in `free` only; the others stay at `th0`
/// (standard error 0).
fn fit_free(
    model: Model,
    window: &'static str,
    pts: &[&Point],
    th0: &[f64],
    free: &[usize],
) -> Fit {
    let nf = free.len();
    let mut th = th0.to_vec();
    let mut lam = 1e-3;
    let ssr = |t: &[f64]| {
        residual_vec(model, t, pts)
            .iter()
            .map(|r| r * r)
            .sum::<f64>()
    };
    let jac = |t: &[f64]| -> Vec<Vec<f64>> {
        let cols: Vec<Vec<f64>> = free
            .iter()
            .map(|&j| {
                let h = 1e-7 * t[j].abs().max(1e-6);
                let (mut tp, mut tm) = (t.to_vec(), t.to_vec());
                tp[j] += h;
                tm[j] -= h;
                residual_vec(model, &tp, pts)
                    .iter()
                    .zip(residual_vec(model, &tm, pts))
                    .map(|(a, b)| (a - b) / (2.0 * h))
                    .collect()
            })
            .collect();
        (0..cols[0].len())
            .map(|i| (0..nf).map(|j| cols[j][i]).collect())
            .collect()
    };
    let normal = |j: &[Vec<f64>]| -> Vec<Vec<f64>> {
        (0..nf)
            .map(|a| {
                (0..nf)
                    .map(|b| j.iter().map(|row| row[a] * row[b]).sum())
                    .collect()
            })
            .collect()
    };
    let mut cur = ssr(&th);
    for _ in 0..500 {
        let j = jac(&th);
        let r = residual_vec(model, &th, pts);
        let jtj = normal(&j);
        let jtr: Vec<f64> = (0..nf)
            .map(|a| j.iter().zip(&r).map(|(row, ri)| row[a] * ri).sum())
            .collect();
        let mut improved = false;
        for _ in 0..30 {
            let mut aug = jtj.clone();
            for (d, row) in aug.iter_mut().enumerate() {
                row[d] *= 1.0 + lam;
            }
            let step = solve_dense(&aug, &jtr.iter().map(|v| -v).collect::<Vec<_>>());
            let mut trial = th.clone();
            for (k, &i) in free.iter().enumerate() {
                trial[i] += step[k];
            }
            let s = ssr(&trial);
            if s < cur {
                let rel = (cur - s) / cur.max(1e-300);
                th = trial;
                cur = s;
                lam = (lam / 3.0).max(1e-12);
                improved = rel > 1e-14;
                break;
            }
            lam *= 4.0;
        }
        if !improved {
            break;
        }
    }
    let r = residual_vec(model, &th, pts);
    let n = r.len();
    let cov = invert(&normal(&jac(&th)));
    let s2 = cur / (n - nf) as f64;
    let mut stderr = vec![0.0; th.len()];
    for (k, &i) in free.iter().enumerate() {
        stderr[i] = (s2 * cov[k][k]).sqrt();
    }
    let nb = model.n_branches();
    let residuals_mhz = pts
        .iter()
        .zip(r.chunks(nb))
        .map(|(p, c)| (p.l_nh, c.iter().map(|v| v * 1e3).collect()))
        .collect();
    Fit {
        model,
        window,
        theta: th,
        stderr,
        rms_mhz: (cur / n as f64).sqrt() * 1e3,
        max_abs_mhz: r.iter().fold(0.0_f64, |a, v| a.max(v.abs())) * 1e3,
        n_data: n,
        residuals_mhz,
    }
}

/// `g` at the crossing (GHz) from a fit: half the minimum splitting.
fn crossing_g_ghz(fit: &Fit) -> f64 {
    match fit.model {
        Model::Rwa2 => fit.theta[2].abs(),
        _ => {
            // Minimum splitting over L_J, golden-section in log L.
            let split = |l: f64| {
                let m = fit.model.predict(&fit.theta, l);
                m[1] - m[0]
            };
            let l_x = (fit.theta[0] / fit.theta[1]).powi(2) - fit.theta[3];
            let (mut lo, mut hi) = (0.9 * l_x, 1.1 * l_x);
            for _ in 0..200 {
                let (a, b) = (lo + 0.382 * (hi - lo), lo + 0.618 * (hi - lo));
                if split(a) < split(b) {
                    hi = b;
                } else {
                    lo = a;
                }
            }
            0.5 * split(0.5 * (lo + hi))
        }
    }
}

// ---------------------------------------------------------------------------
// Eigen sweep.
// ---------------------------------------------------------------------------

fn eigen_sweep(fx: &TransmonFixture, lj_nh: &[f64]) -> Vec<Point> {
    let edges = fx.mesh.edges();
    let (tet_edge_idx, tet_edge_sign) = edge_tables(&fx.mesh);
    let metal = fx.metal_triangles();
    let exterior = fx.exterior_boundary_triangles();
    let interior_mask =
        pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let eps = fx.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&tet_edge_idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(&fx.mesh, &device());
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu = vec![identity; fx.mesh.n_tets()];
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t,
        tets_t,
        &tet_edge_sign,
        &scatter,
        &eps,
        &nu,
    );
    let k_vals = vals_to_host(sys.k_re_vals);
    let m_vals = vals_to_host(sys.m_re_vals);
    for (name, v) in [
        ("K", vals_to_host(sys.k_im_vals)),
        ("M", vals_to_host(sys.m_im_vals)),
    ] {
        let m = v.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
        assert!(m <= 1e-9, "{name} imaginary part {m} is not negligible");
    }
    let jport = fx.lumped_element_port();
    let sigma = lambda_shift_for_frequency_hz(SIGMA_GHZ * 1e9, M_PER_UNIT);
    lj_nh
        .iter()
        .map(|&l| {
            let t = Instant::now();
            let element =
                ReactiveElementNatural::from_si(l * 1e-9, JUNCTION_CAPACITANCE_F, M_PER_UNIT);
            let pencil = TransmonPencil {
                scatter: &scatter,
                k_vals: &k_vals,
                m_vals: &m_vals,
                edges: &edges,
                mesh: &fx.mesh,
                shunt: LumpedReactiveShunt {
                    faces: &jport.faces,
                    length: jport.length,
                    width: jport.width,
                    element,
                },
                interior_mask: &interior_mask,
            };
            let (modes, diag) = solve_transmon_eigenmodes_projected_with_port_model(
                &pencil,
                JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
                sigma,
                N_MODES,
                M_PER_UNIT,
            )
            .expect("A2 bulk-projected eigensolve");
            assert_eq!(modes.len(), N_MODES);
            let mut order: Vec<usize> = (0..N_MODES).collect();
            order.sort_by(|&i, &j| modes[i].frequency_hz.total_cmp(&modes[j].frequency_hz));
            let pt = Point {
                l_nh: l,
                f: order.iter().map(|&i| modes[i].frequency_ghz()).collect(),
                p: order.iter().map(|&i| modes[i].participation).collect(),
                res: order.iter().map(|&i| diag.mode_residual_rels[i]).collect(),
                div: order
                    .iter()
                    .map(|&i| diag.mode_divergence_ratios[i])
                    .collect(),
                solve_s: t.elapsed().as_secs_f64(),
            };
            println!(
                "  L_J {l:7.3} nH: f = [{:.6}, {:.6}, {:.6}] GHz, p = [{:.4}, {:.4}, {:.2e}], \
                 max residual {:.1e} ({:.1}s)",
                pt.f[0],
                pt.f[1],
                pt.f[2],
                pt.p[0],
                pt.p[1],
                pt.p[2],
                pt.res.iter().fold(0.0_f64, |a, &b| a.max(b)),
                pt.solve_s
            );
            assert!(
                pt.res.iter().all(|&r| r <= 1e-6),
                "residuals {:?} at L_J {l}",
                pt.res
            );
            pt
        })
        .collect()
}

/// Re-read the `[sweep]` table of an earlier output (development only).
fn reuse_sweep(path: &Path) -> Vec<Point> {
    let doc: toml::Value =
        toml::from_str(&fs::read_to_string(path).expect("read sweep")).expect("parse sweep");
    let sw = &doc["sweep"];
    let col = |k: &str| -> Vec<f64> {
        sw[k]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_float().expect("float"))
            .collect()
    };
    let l = col("l_j_nh");
    assert_eq!(l.len(), SWEEP_LJ_NH.len(), "sweep list changed");
    let (f0, f1, f2) = (col("f_lower_ghz"), col("f_upper_ghz"), col("f_third_ghz"));
    let (p0, p1, p2) = (col("p_lower"), col("p_upper"), col("p_third"));
    (0..l.len())
        .map(|i| Point {
            l_nh: l[i],
            f: vec![f0[i], f1[i], f2[i]],
            p: vec![p0[i], p1[i], p2[i]],
            res: vec![0.0; 3],
            div: vec![0.0; 3],
            solve_s: 0.0,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Static region-split proxy.
// ---------------------------------------------------------------------------

/// One sheet region: name, node set, triangles.
type Region = (String, Vec<u32>, Vec<[u32; 3]>);

/// The resonator split of the ground conductor (base-mesh units).
struct ResonatorSplit {
    /// Regions in claim order: ground, island, feedline, claw, trace.
    regions: Vec<Region>,
    n_trace_triangles: usize,
    n_trace_nodes: usize,
    trace_area_um2: f64,
    cut_nodes: Vec<[f64; 2]>,
    bbox_um: [f64; 4],
}

fn resonator_split(fx: &TransmonFixture) -> ResonatorSplit {
    let comps = fx.split_metal_conductors();
    let get = |r: MetalRole| comps.iter().find(|c| c.role == r).expect("conductor");
    let (gnd, isl, feed) = (
        get(MetalRole::Ground),
        get(MetalRole::Island),
        get(MetalRole::Feedline),
    );
    let nd = |n: u32| fx.mesh.nodes[n as usize];
    let mut edge_tris: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, t) in gnd.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edge_tris.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let centroid = |t: &[u32; 3]| {
        let (p, q, r) = (nd(t[0]), nd(t[1]), nd(t[2]));
        [(p[0] + q[0] + r[0]) / 3.0, (p[1] + q[1] + r[1]) / 3.0]
    };
    let beyond_short =
        |c: [f64; 2]| c[0] < SHORT_X_UM && c[1] > SHORT_Y_UM.0 && c[1] < SHORT_Y_UM.1;
    let (wx, wy) = SEED_WINDOW_UM;
    let seed = gnd
        .triangles
        .iter()
        .position(|t| {
            let c = centroid(t);
            c[0] > wx[0] && c[0] < wx[1] && c[1] > wy[0] && c[1] < wy[1]
        })
        .expect("a ground triangle in the trace seed window");
    let mut flooded = vec![false; gnd.triangles.len()];
    flooded[seed] = true;
    let mut stack = vec![seed];
    while let Some(i) = stack.pop() {
        let t = gnd.triangles[i];
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            for &j in &edge_tris[&(a.min(b), a.max(b))] {
                if !flooded[j] && !beyond_short(centroid(&gnd.triangles[j])) {
                    flooded[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    let pick = |want: bool| -> Vec<[u32; 3]> {
        gnd.triangles
            .iter()
            .zip(&flooded)
            .filter(|(_, f)| **f == want)
            .map(|(t, _)| *t)
            .collect()
    };
    let (trace_tris, ground_tris) = (pick(true), pick(false));
    let ground_tri_nodes: BTreeSet<u32> = ground_tris.iter().flatten().copied().collect();
    let trace_nodes: BTreeSet<u32> = trace_tris
        .iter()
        .flatten()
        .copied()
        .filter(|n| !ground_tri_nodes.contains(n))
        .collect();
    let cut_nodes: Vec<[f64; 2]> = trace_tris
        .iter()
        .flatten()
        .copied()
        .filter(|n| ground_tri_nodes.contains(n))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|n| [nd(n)[0], nd(n)[1]])
        .collect();
    let area: f64 = trace_tris
        .iter()
        .map(|t| {
            let (p, q, r) = (nd(t[0]), nd(t[1]), nd(t[2]));
            0.5 * ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])).abs()
        })
        .sum();
    let mut bbox = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for &n in &trace_nodes {
        let p = nd(n);
        bbox = [
            bbox[0].min(p[0]),
            bbox[1].max(p[0]),
            bbox[2].min(p[1]),
            bbox[3].max(p[1]),
        ];
    }
    let claw: BTreeSet<u32> = trace_nodes
        .iter()
        .copied()
        .filter(|&n| nd(n)[1] < CLAW_Y_UM)
        .collect();
    let (claw_tris, rest_tris): (Vec<[u32; 3]>, Vec<[u32; 3]>) = trace_tris
        .iter()
        .partition(|t| t.iter().all(|n| claw.contains(n)));
    let rest: Vec<u32> = trace_nodes
        .iter()
        .copied()
        .filter(|n| !claw.contains(n))
        .collect();
    let ground_nodes: Vec<u32> = gnd
        .nodes
        .iter()
        .copied()
        .filter(|n| !trace_nodes.contains(n))
        .collect();
    ResonatorSplit {
        regions: vec![
            ("ground".into(), ground_nodes, ground_tris),
            ("island".into(), isl.nodes.clone(), isl.triangles.clone()),
            (
                "feedline".into(),
                feed.nodes.clone(),
                feed.triangles.clone(),
            ),
            ("claw".into(), claw.into_iter().collect(), claw_tris),
            ("trace".into(), rest, rest_tris),
        ],
        n_trace_triangles: trace_tris.len(),
        n_trace_nodes: trace_nodes.len(),
        trace_area_um2: area,
        cut_nodes,
        bbox_um: bbox,
    }
}

/// Static capacitances of one refinement level (fF).
#[derive(Clone, Debug)]
struct StaticLevel {
    n_tets: usize,
    c_sigma: f64,
    c_g: f64,
    c_g_claw: f64,
    c_rr: f64,
    pcg_iterations: Vec<usize>,
    solve_s: f64,
}

impl StaticLevel {
    fn beta(&self) -> f64 {
        self.c_g / self.c_sigma
    }
    fn k_static(&self) -> f64 {
        let c_q = self.c_sigma + JUNCTION_CAPACITANCE_F * 1e15;
        self.c_g / (c_q * 0.5 * self.c_rr).sqrt()
    }
}

/// Floating-feedline reduction of the Maxwell matrix onto
/// {island, resonator (claw + trace)}: (C_Σ, C_g, C_g(claw only), C_rr) in fF.
fn reduce(cm: &CapacitanceMatrix) -> (f64, f64, f64, f64) {
    let ix = |n: &str| cm.index_of(n).expect("conductor");
    let (i, f, c, t) = (ix("island"), ix("feedline"), ix("claw"), ix("trace"));
    let red = |a: usize, b: usize| cm.c[a][b] - cm.c[a][f] * cm.c[f][b] / cm.c[f][f];
    let c_sigma = red(i, i);
    let c_g_claw = -red(i, c);
    let c_g = -(red(i, c) + red(i, t));
    let c_rr = red(c, c) + red(t, t) + 2.0 * red(c, t);
    (c_sigma * 1e15, c_g * 1e15, c_g_claw * 1e15, c_rr * 1e15)
}

fn static_levels(
    fx: &TransmonFixture,
    split: &ResonatorSplit,
    n_levels: usize,
) -> Vec<StaticLevel> {
    let mut mesh = fx.mesh.clone();
    for n in mesh.nodes.iter_mut() {
        for x in n.iter_mut() {
            *x *= M_PER_UNIT;
        }
    }
    let lab = sapphire_eps_lab();
    let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut eps: Vec<[[f64; 3]; 3]> = fx
        .tet_physical_tags
        .iter()
        .map(|&t| if t == fx.tags.substrate { lab } else { id })
        .collect();
    let mut regions = split.regions.clone();
    let mut out = Vec::new();
    for level in 0..n_levels {
        if level > 0 {
            // Lift every sheet region; a node shared by two regions goes to
            // the first (ground claims the cut at the short, the claw the
            // claw/trace boundary).
            let r = red_refine(&mesh);
            assert_eq!(r.n_flipped, 0);
            let mut claimed: BTreeSet<u32> = BTreeSet::new();
            for (_, nodes, tris) in regions.iter_mut() {
                let lifted = r.lift_sheet_nodes(nodes, tris);
                *nodes = lifted
                    .into_iter()
                    .filter(|n| !claimed.contains(n))
                    .collect();
                claimed.extend(nodes.iter().copied());
                *tris = r.lift_triangles(tris);
            }
            eps = r.lift_per_tet(&eps);
            mesh = r.mesh;
        }
        let t = Instant::now();
        let conds: Vec<Electrode> = regions[1..]
            .iter()
            .map(|(n, nodes, _)| Electrode {
                name: n.clone(),
                nodes: nodes.clone(),
                voltage: 0.0,
            })
            .collect();
        let ground = &regions[0].1;
        let rho = vec![0.0; mesh.n_tets()];
        let sys = assemble_electrostatic_tensor(&mesh, &eps, &rho, &conds, ground)
            .expect("electrostatic assembly");
        let (cm, rep) = extract_capacitance_amg_pcg(&sys, &conds, ground, AmgPcgOptions::default())
            .expect("AMG-PCG capacitance extraction");
        let (c_sigma, c_g, c_g_claw, c_rr) = reduce(&cm);
        let lvl = StaticLevel {
            n_tets: mesh.n_tets(),
            c_sigma,
            c_g,
            c_g_claw,
            c_rr,
            pcg_iterations: rep.iterations.clone(),
            solve_s: t.elapsed().as_secs_f64(),
        };
        println!(
            "  static level {level}: {} tets, C_Σ {c_sigma:.6}, C_g {c_g:.6} (claw {c_g_claw:.6}), \
             C_rr {c_rr:.4} fF, k_s {:.6} ({:.1}s)",
            lvl.n_tets,
            lvl.k_static(),
            lvl.solve_s
        );
        out.push(lvl);
    }
    out
}

/// Richardson-type extrapolation of a three-level series with a fixed
/// halving: (observed order, geometric-tail limit, order-1 limit).
fn extrapolate(v: [f64; 3]) -> (f64, f64, f64) {
    let (d1, d2) = (v[1] - v[0], v[2] - v[1]);
    let ratio = d1 / d2;
    (ratio.log2(), v[2] + d2 / (ratio - 1.0), v[2] + d2)
}

// ---------------------------------------------------------------------------
// Palace.
// ---------------------------------------------------------------------------

/// Re f (GHz) of every converged mode in a Palace `eig.csv`.
fn palace_eig(path: &Path) -> Vec<f64> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .filter_map(|l| l.split(',').nth(1))
        .map(|s| s.trim().parse::<f64>().expect("Re f"))
        .collect()
}

/// p[1] per mode from a Palace `port-EPR.csv`.
fn palace_epr(path: &Path) -> Vec<f64> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(1)
        .filter_map(|l| l.split(',').nth(1))
        .map(|s| s.trim().parse::<f64>().expect("p[1]"))
        .collect()
}

// ---------------------------------------------------------------------------
// Output helpers.
// ---------------------------------------------------------------------------

fn floor_dp(x: f64, dp: i32) -> f64 {
    let s = 10f64.powi(dp);
    (x * s).floor() / s
}

fn ceil_dp(x: f64, dp: i32) -> f64 {
    let s = 10f64.powi(dp);
    (x * s).ceil() / s
}

fn arr(v: &[f64], prec: usize) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| format!("{x:.prec$}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn arr_e(v: &[f64]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| format!("{x:.2e}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// The bare-qubit anchor for a coupling `k` (Maxwell form) and its offset
/// from the dressed anchor: (f_q Hz, f_r Hz, C_Σ fF, shift fF).
fn bare_for_k(k: f64) -> (f64, f64, f64, f64) {
    let a = TransmonAnchor::devicelayout_bare_with_coupling(k);
    let (_, f_r) = coupled_lc_bare_pair_hz(
        DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
        DEVICELAYOUT_RESONATOR_FREQUENCY_HZ,
        k,
    );
    let dressed = TransmonAnchor::devicelayout().c_sigma_farad * 1e15;
    let c = a.c_sigma_farad * 1e15;
    (a.frequency_hz, f_r, c, dressed - c)
}

/// Issue-recipe (rotating-wave) bare qubit frequency from the dressed pair
/// and an absolute `g` (Hz).
fn rwa_bare_qubit_hz(g_hz: f64) -> f64 {
    let (f1, f2) = (
        DEVICELAYOUT_QUBIT_FREQUENCY_HZ,
        DEVICELAYOUT_RESONATOR_FREQUENCY_HZ,
    );
    let half = 0.5 * (f2 - f1);
    0.5 * (f1 + f2) - (half * half - g_hz * g_hz).sqrt()
}

/// The bare qubit frequency (GHz) of DeviceLayout's dressed pair in a
/// multi-node Maxwell model: the λ/4 fundamental at `f_r` with coupling `k`,
/// and resonator harmonics at `ratio · f_r` with coupling `k_ratio · k`, one
/// node each, uncoupled to each other. Newton on `(f_q, f_r)` from the
/// two-node inversion.
fn bare_multimode_ghz(k: f64, harmonics: &[(f64, f64)]) -> f64 {
    let (d1, d2) = (
        DEVICELAYOUT_QUBIT_FREQUENCY_HZ / 1e9,
        DEVICELAYOUT_RESONATOR_FREQUENCY_HZ / 1e9,
    );
    let (q0, r0) = coupled_lc_bare_pair_hz(d1 * 1e9, d2 * 1e9, k);
    let mut x = [q0 / 1e9, r0 / 1e9];
    let n = 2 + harmonics.len();
    let mut kk = vec![vec![0.0; n]; n];
    kk[0][1] = k;
    kk[1][0] = k;
    for (i, &(_, kr)) in harmonics.iter().enumerate() {
        kk[0][2 + i] = kr * k;
        kk[2 + i][0] = kr * k;
    }
    let res = |x: [f64; 2]| {
        let mut f = vec![x[0], x[1]];
        f.extend(harmonics.iter().map(|&(r, _)| r * x[1]));
        let m = maxwell_modes(&f, &kk).0;
        [m[0] - d1, m[1] - d2]
    };
    for _ in 0..50 {
        let r = res(x);
        if r[0].abs().max(r[1].abs()) < 1e-13 {
            break;
        }
        let h = 1e-7;
        let j: Vec<Vec<f64>> = (0..2)
            .map(|a| {
                (0..2)
                    .map(|b| {
                        let mut xp = x;
                        xp[b] += h;
                        (res(xp)[a] - r[a]) / h
                    })
                    .collect()
            })
            .collect();
        let step = solve_dense(&j, &[-r[0], -r[1]]);
        x = [x[0] + step[0], x[1] + step[1]];
    }
    x[0]
}

/// `C_Σ` (fF) of a bare qubit frequency (GHz) with the anchor's `L_J`, `C_J`.
fn c_sigma_of_ghz(f_ghz: f64) -> f64 {
    TransmonAnchor::from_harmonic(f_ghz * 1e9, JUNCTION_INDUCTANCE_H, JUNCTION_CAPACITANCE_F)
        .c_sigma_farad
        * 1e15
}

fn main() {
    let t_all = Instant::now();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = std::env::var("TRANSMON_COUPLING_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("benchmarks/transmon_coupling"));
    let fx = read_transmon_smoke_fixture().expect("transmon fixture");
    let backend = BackendInfo::of::<B>(&device());

    // ---- 1. Eigen sweep. ----
    println!("eigen sweep (A2 shunt, bulk projector, σ = {SIGMA_GHZ} GHz, N = {N_MODES}):");
    let t_eig = Instant::now();
    // Development only: `TRANSMON_COUPLING_REUSE_SWEEP=<results.toml>` re-reads
    // the [sweep] of an earlier output instead of solving. The committed
    // artifact records `sweep_source = "solved"`, and tests check it.
    let reuse = std::env::var("TRANSMON_COUPLING_REUSE_SWEEP").ok();
    let pts = match &reuse {
        Some(path) => reuse_sweep(Path::new(path)),
        None => eigen_sweep(&fx, &SWEEP_LJ_NH),
    };
    let eig_s = t_eig.elapsed().as_secs_f64();

    // ---- 2. Fits. ----
    let all: Vec<&Point> = pts.iter().collect();
    let win: Vec<&Point> = pts
        .iter()
        .filter(|p| p.l_nh >= WINDOW_LJ_NH.0 && p.l_nh <= WINDOW_LJ_NH.1)
        .collect();
    // Starting values: a from the 14.86 nH qubit, f_r from 4 nH, g from the
    // smallest splitting.
    let base = pts
        .iter()
        .find(|p| p.l_nh == 14.86)
        .expect("14.86 nH point");
    let a0 = base.f[0] * 14.86_f64.sqrt();
    let fr0 = pts[0].f[0];
    let min_split = pts
        .iter()
        .map(|p| p.f[1] - p.f[0])
        .fold(f64::INFINITY, f64::min);
    let g0 = 0.5 * min_split;
    let k0 = 2.0 * g0 / fr0;
    let mut fits = Vec::new();
    for (wname, set) in [("all", &all), ("window", &win)] {
        let m2 = fit(Model::Maxwell2, wname, set, &[a0, fr0, k0, 0.0]);
        let r2 = fit(Model::Rwa2, wname, set, &[a0, fr0, g0, 0.0]);
        let th3 = [
            m2.theta[0],
            m2.theta[1],
            m2.theta[2],
            m2.theta[3],
            15.465,
            m2.theta[2],
        ];
        let m3 = fit(Model::Maxwell3, wname, set, &th3);
        fits.extend([m2, r2, m3]);
    }
    for f in &fits {
        println!(
            "  fit {} / {}: θ = {:?} ± {:?}, rms {:.4} MHz, max {:.4} MHz, g_x {:.4} MHz",
            f.model.name(),
            f.window,
            f.theta,
            f.stderr,
            f.rms_mhz,
            f.max_abs_mhz,
            crossing_g_ghz(f) * 1e3
        );
    }
    let primary = &fits[0];
    let k_h = primary.theta[2];

    // Participation check of the primary fit: predicted junction share of
    // the lower mode against the solved one (normalized by the pair sum).
    let part_check: Vec<(f64, f64, f64)> = pts
        .iter()
        .map(|p| {
            let fq = primary.theta[0] / (p.l_nh + primary.theta[3]).sqrt();
            let k = vec![vec![0.0, k_h], vec![k_h, 0.0]];
            let (_, shares) = maxwell_modes(&[fq, primary.theta[1]], &k);
            (p.l_nh, p.p[0] / (p.p[0] + p.p[1]), shares[0][0])
        })
        .collect();
    let part_max_dev = part_check
        .iter()
        .map(|(_, a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);

    // ---- 3. Palace. ----
    let pdir = repo.join(PALACE_DIR);
    let mut palace_rows = Vec::new();
    for &(name, level, l) in &PALACE_CELLS {
        let f = palace_eig(&pdir.join(name).join("eig.csv"));
        let p = palace_epr(&pdir.join(name).join("port-EPR.csv"));
        let geode = pts
            .iter()
            .find(|q| (q.l_nh - l).abs() < 1e-9)
            .expect("geode point at the Palace L_J");
        let rel: Vec<f64> = (0..2).map(|b| (geode.f[b] - f[b]) / f[b]).collect();
        palace_rows.push((name, level, l, f, p, geode.f.clone(), rel));
    }
    let palace_max_rel = palace_rows
        .iter()
        .flat_map(|r| r.6.iter().map(|v| v.abs()))
        .fold(0.0_f64, f64::max);
    let palace_h_pts: Vec<Point> = palace_rows
        .iter()
        .filter(|r| r.1 == 0)
        .map(|r| Point {
            l_nh: r.2,
            f: r.3[..2].to_vec(),
            p: r.4[..2].iter().map(|x| x.abs()).collect(),
            res: vec![],
            div: vec![],
            solve_s: 0.0,
        })
        .collect();
    let palace_h_fit = {
        let refs: Vec<&Point> = palace_h_pts.iter().collect();
        fit_free(
            Model::Maxwell2,
            "palace_h",
            &refs,
            &primary.theta,
            &[0, 1, 2],
        )
    };
    let palace_split_rel: Vec<f64> = palace_rows
        .iter()
        .map(|r| ((r.5[1] - r.5[0]) - (r.3[1] - r.3[0])) / (r.3[1] - r.3[0]))
        .collect();
    let ams_rel = {
        let a = palace_eig(&pdir.join(PALACE_AMS_CONTROL.0).join("eig.csv"));
        let d = palace_eig(&pdir.join(PALACE_AMS_CONTROL.1).join("eig.csv"));
        (0..2)
            .map(|b| ((a[b] - d[b]) / d[b]).abs())
            .fold(0.0_f64, f64::max)
    };
    let h2_pts: Vec<Point> = PALACE_H2_CELLS
        .iter()
        .map(|&(name, l)| {
            let f = palace_eig(&pdir.join(name).join("eig.csv"));
            let p = palace_epr(&pdir.join(name).join("port-EPR.csv"));
            Point {
                l_nh: l,
                f: f[..3.min(f.len())].to_vec(),
                p: p[..3.min(p.len())].to_vec(),
                res: vec![],
                div: vec![],
                solve_s: 0.0,
            }
        })
        .collect();
    let h2_fit = if h2_pts.len() >= 4 {
        let refs: Vec<&Point> = h2_pts.iter().collect();
        let base2 = h2_pts
            .iter()
            .max_by(|a, b| a.l_nh.total_cmp(&b.l_nh))
            .unwrap();
        let a2 = base2.f[0] * base2.l_nh.sqrt();
        let fr2 = h2_pts
            .iter()
            .min_by(|a, b| a.l_nh.total_cmp(&b.l_nh))
            .unwrap()
            .f[0];
        Some(fit(
            Model::Maxwell2,
            "palace_h2",
            &refs,
            &[a2, fr2, k_h, primary.theta[3]],
        ))
    } else {
        None
    };

    // ---- 4. Static proxy. ----
    println!("static region-split proxy (tensor eps, P1, AMG-PCG):");
    let split = resonator_split(&fx);
    let t_st = Instant::now();
    let st = static_levels(&fx, &split, 3);
    let static_s = t_st.elapsed().as_secs_f64();
    let ks: [f64; 3] = std::array::from_fn(|i| st[i].k_static());
    let betas: [f64; 3] = std::array::from_fn(|i| st[i].beta());
    let (ks_order, ks_geo, ks_p1) = extrapolate(ks);
    let (beta_order, beta_geo, beta_p1) = extrapolate(betas);
    let proxy_ratio_h = k_h / ks[0];

    // ---- 5. Coupling estimates, bare anchor and verdicts. ----
    // Converged-k estimates (all inferences; k rises at every measured level):
    //  e1, e2: two-level Richardson of the eigen k (h: geode, h/2: Palace)
    //          at the static proxy's observed order and at order 1;
    //  e3, e4: the static proxy's order-1 and geometric-tail limits times the
    //          eigen/static ratio measured at h/2.
    // The library constants follow from them: K = midpoint of [min, max],
    // band = [Palace h/2 k floored, static geometric-tail limit (ratio 1)
    // ceiled], all to 3 significant figures (pinned by
    // tests/transmon_eigenmode.rs::coupling_constants_follow_the_committed_estimates).
    let k_h2 = h2_fit.as_ref().map(|f| f.theta[2]).expect("Palace h/2 fit");
    let ratio_h2 = k_h2 / ks[1];
    let rich = |order: f64| k_h2 + (k_h2 - k_h) / (2f64.powf(order) - 1.0);
    let estimates = [
        rich(ks_order),
        rich(1.0),
        ratio_h2 * ks_p1,
        ratio_h2 * ks_geo,
    ];
    let e_min = estimates.iter().copied().fold(f64::INFINITY, f64::min);
    let e_max = estimates.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sig3 = |x: f64, mode: i32| {
        let s = 10f64.powi(3 - 1 - x.log10().floor() as i32);
        match mode {
            -1 => (x * s).floor() / s,
            1 => (x * s).ceil() / s,
            _ => (x * s).round() / s,
        }
    };
    let k_rule = (
        sig3(0.5 * (e_min + e_max), 0),
        sig3(k_h2, -1),
        sig3(ks_geo, 1),
    );
    let (k_lo, k_hi) = DEVICELAYOUT_COUPLING_K_BAND;
    assert!(
        (k_rule.0 - DEVICELAYOUT_COUPLING_K).abs() < 1e-12
            && (k_rule.1 - k_lo).abs() < 1e-12
            && (k_rule.2 - k_hi).abs() < 1e-12,
        "quantum::transmon coupling constants ({DEVICELAYOUT_COUPLING_K}, {k_lo}, {k_hi}) do not \
         follow the estimates: want {k_rule:?}"
    );
    let central = bare_for_k(DEVICELAYOUT_COUPLING_K);
    let lo = bare_for_k(k_lo);
    let same_mesh = bare_for_k(k_h);
    let h2_mesh = bare_for_k(k_h2);
    // Round trip of the inversion.
    let (rt1, rt2) = coupled_lc_dressed_pair_hz(central.0, central.1, DEVICELAYOUT_COUPLING_K);
    assert!((rt1 - DEVICELAYOUT_QUBIT_FREQUENCY_HZ).abs() < 1.0);
    assert!((rt2 - DEVICELAYOUT_RESONATOR_FREQUENCY_HZ).abs() < 1.0);
    // Resonator harmonics: the 3λ/4 node from the all-window three-mode fit
    // (measured), and the ideal λ/4 series 5λ/4 ... 15λ/4 with the same
    // frequency and coupling ratios (an inference).
    let m3 = fits
        .iter()
        .find(|f| f.model == Model::Maxwell3 && f.window == "all")
        .expect("three-mode fit");
    let (k3_over_k, r3) = (m3.theta[5] / m3.theta[2], m3.theta[4] / m3.theta[1]);
    let third_only = [(r3, k3_over_k)];
    let odd: Vec<(f64, f64)> = (1..=7)
        .map(|j| ((2 * j + 1) as f64 * r3 / 3.0, k3_over_k))
        .collect();
    let c3 = c_sigma_of_ghz(bare_multimode_ghz(DEVICELAYOUT_COUPLING_K, &third_only));
    let c_odd = c_sigma_of_ghz(bare_multimode_ghz(DEVICELAYOUT_COUPLING_K, &odd));
    let fq_odd_hi = bare_multimode_ghz(k_hi, &odd);
    let c_odd_hi = c_sigma_of_ghz(fq_odd_hi);
    // Issue recipe (RWA) with this mesh's crossing g as an absolute frequency.
    let g_x_hz = crossing_g_ghz(primary) * 1e9;
    let rwa_abs = rwa_bare_qubit_hz(g_x_hz);
    let rwa_abs_c = c_sigma_of_ghz(rwa_abs / 1e9);
    let dressed_c = TransmonAnchor::devicelayout().c_sigma_farad * 1e15;
    // Target band (fF): lowest from the largest k with every odd harmonic,
    // highest from the smallest k with the two-node model.
    let target_band = (c_odd_hi, lo.2);
    let verdict = |b: (f64, f64)| -> &'static str {
        if b.0 > target_band.1 {
            "above"
        } else if b.1 < target_band.0 {
            "below"
        } else if b.0 <= target_band.0 && b.1 >= target_band.1 {
            "contains"
        } else {
            "overlaps"
        }
    };

    // ---- Emit. ----
    let mut t = String::with_capacity(32 * 1024);
    t.push_str(
        "# Auto-generated by `cargo run -p geode-core --release --example transmon_coupling_g`.\n",
    );
    t.push_str("# Do NOT edit by hand; regenerate after any intentional change.\n");
    t.push_str("# Issue #1078. Checked by tests/transmon_eigenmode.rs (coupling_* tests).\n\n");
    t.push_str("[meta]\n");
    t.push_str("description = \"Transmon qubit-resonator coupling from an avoided-crossing L_J sweep on the committed 133k-tet DeviceLayout SingleTransmon fixture (A2 port-voltage shunt, bulk projector), fitted with the capacitively coupled two-node LC circuit; Palace cross-check on the same mesh and a Palace L_J scan on one uniform refinement; a region-split static proxy (resonator trace as its own electrode) at h, h/2, h/4; and the bare-qubit C_Sigma anchor derived from DeviceLayout's dressed pair [4.14, 5.591] GHz.\"\n");
    t.push_str("issue = 1078\n");
    t.push_str("fixture = \"crates/geode-core/tests/fixtures/transmon_smoke.msh\"\n");
    let _ = writeln!(t, "n_tets = {}", fx.mesh.n_tets());
    let _ = writeln!(t, "generated_at_commit = \"{}\"", git_head());
    backend.push_meta(&mut t);
    let _ = writeln!(
        t,
        "sweep_source = \"{}\"",
        if reuse.is_some() { "reused" } else { "solved" }
    );
    let _ = writeln!(t, "eigen_wall_s = {eig_s:.1}");
    let _ = writeln!(t, "static_wall_s = {static_s:.1}");
    let _ = writeln!(t, "wall_clock_s = {:.1}", t_all.elapsed().as_secs_f64());
    t.push_str(
        "host = \"local CPU (Apple M3 Ultra, 28 cores); local docker for Palace; no cloud\"\n\n",
    );

    t.push_str("[sweep]\n");
    t.push_str("solver = \"solve_transmon_eigenmodes_projected_with_port_model, JunctionPortModel::PortVoltage (A2, #1044), plain bulk projector, one faer LU of K - sigma M per L_J\"\n");
    let _ = writeln!(t, "sigma_ghz = {SIGMA_GHZ}");
    let _ = writeln!(t, "n_modes = {N_MODES}");
    let _ = writeln!(
        t,
        "junction_capacitance_ff = {}",
        JUNCTION_CAPACITANCE_F * 1e15
    );
    t.push_str(
        "participation = \"geode junction participation (V^2/L~) / x^T K_tot x of each mode\"\n",
    );
    let _ = writeln!(t, "l_j_nh = {}", arr(&SWEEP_LJ_NH, 3));
    let col = |b: usize| pts.iter().map(|p| p.f[b]).collect::<Vec<_>>();
    let pcol = |b: usize| pts.iter().map(|p| p.p[b]).collect::<Vec<_>>();
    let _ = writeln!(t, "f_lower_ghz = {}", arr(&col(0), 9));
    let _ = writeln!(t, "f_upper_ghz = {}", arr(&col(1), 9));
    let _ = writeln!(
        t,
        "f_third_ghz = {}  # the resonator's 3 lambda/4 harmonic",
        arr(&col(2), 9)
    );
    let _ = writeln!(t, "p_lower = {}", arr(&pcol(0), 6));
    let _ = writeln!(t, "p_upper = {}", arr(&pcol(1), 6));
    let _ = writeln!(t, "p_third = {}", arr_e(&pcol(2)));
    let maxres: Vec<f64> = pts
        .iter()
        .map(|p| p.res.iter().fold(0.0_f64, |a, &b| a.max(b)))
        .collect();
    let maxdiv: Vec<f64> = pts
        .iter()
        .map(|p| p.div.iter().fold(0.0_f64, |a, &b| a.max(b)))
        .collect();
    let _ = writeln!(t, "max_residual_rel = {}", arr_e(&maxres));
    let _ = writeln!(t, "max_divergence_ratio = {}", arr_e(&maxdiv));
    let _ = writeln!(t, "min_sampled_splitting_mhz = {:.4}", min_split * 1e3);
    let _ = writeln!(
        t,
        "solve_s = {}\n",
        arr(&pts.iter().map(|p| p.solve_s).collect::<Vec<_>>(), 1)
    );

    for f in &fits {
        let _ = writeln!(t, "[fit.{}_{}]", f.model.name(), f.window);
        let desc = match f.model {
            Model::Maxwell2 => {
                "two-node LC circuit, Maxwell form: (f_q^2 - f^2)(f_r^2 - f^2) = k^2 f^4, f_q = a / sqrt(L_J + L_s)"
            }
            Model::Rwa2 => {
                "rotating-wave two-mode formula with constant g: f = (f_q + f_r)/2 +- sqrt(((f_q - f_r)/2)^2 + g^2), f_q = a / sqrt(L_J + L_s)"
            }
            Model::Maxwell3 => {
                "three-node Maxwell circuit (qubit, lambda/4, 3 lambda/4; no resonator-harmonic coupling), all three modes; f_q = a / sqrt(L_J + L_s)"
            }
        };
        let _ = writeln!(t, "model = \"{desc}\"");
        let _ = writeln!(
            t,
            "window_l_j_nh = {}",
            if f.window == "all" {
                arr(&[SWEEP_LJ_NH[0], SWEEP_LJ_NH[SWEEP_LJ_NH.len() - 1]], 3)
            } else {
                arr(&[WINDOW_LJ_NH.0, WINDOW_LJ_NH.1], 3)
            }
        );
        let _ = writeln!(t, "n_data = {}", f.n_data);
        for (i, name) in f.model.params().iter().enumerate() {
            let _ = writeln!(t, "{name} = {:.9}", f.theta[i]);
            let _ = writeln!(t, "{name}_stderr = {:.3e}", f.stderr[i]);
        }
        let _ = writeln!(
            t,
            "g_crossing_mhz = {:.4}  # half the minimum splitting of the fitted model",
            crossing_g_ghz(f) * 1e3
        );
        let c_q =
            1.0 / (4.0 * std::f64::consts::PI.powi(2) * f.theta[0].powi(2) * 1e18 * 1e-9) * 1e15;
        let _ = writeln!(
            t,
            "c_q_ff = {c_q:.4}  # 1/((2 pi a)^2): the fitted qubit node capacitance C_Sigma + C_J"
        );
        let _ = writeln!(t, "rms_residual_mhz = {:.4}", f.rms_mhz);
        let _ = writeln!(t, "max_abs_residual_mhz = {:.4}", f.max_abs_mhz);
        let nb = f.model.n_branches();
        for b in 0..nb {
            let v: Vec<f64> = f.residuals_mhz.iter().map(|(_, r)| r[b]).collect();
            let _ = writeln!(t, "residual_branch{b}_mhz = {}", arr(&v, 4));
        }
        t.push('\n');
    }

    t.push_str("[participation_check]\n");
    t.push_str("note = \"lower-mode junction share p_lower/(p_lower + p_upper) from the solves against the qubit node's inductive-energy share of the lower mode in the primary fit (an eigenvector check; the fit used only frequencies)\"\n");
    let _ = writeln!(
        t,
        "solved = {}",
        arr(&part_check.iter().map(|x| x.1).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(
        t,
        "model = {}",
        arr(&part_check.iter().map(|x| x.2).collect::<Vec<_>>(), 6)
    );
    let _ = writeln!(t, "max_abs_deviation = {part_max_dev:.4}\n");

    t.push_str("[palace]\n");
    let _ = writeln!(t, "results_dir = \"{PALACE_DIR}\"");
    t.push_str("image = \"palace:cpu-1043 (reference/palace/docker/Dockerfile, built locally; Palace fba6a5b)\"\n");
    t.push_str("junction_model = \"Palace LumpedPort (distributed S_Gamma), L_J swept, C_J = 5.5 fF; geode uses A2. The two agree on the qubit and resonator to 4 decimals at 14.86 nH ([port_voltage_a2] of benchmarks/transmon_eigen/results.toml)\"\n");
    let _ = writeln!(
        t,
        "max_rel_diff_h = {palace_max_rel:.3e}  # geode vs Palace, both hybridized branches, h cells"
    );
    let _ = writeln!(
        t,
        "splitting_rel_diff_h = {}  # geode vs Palace splitting f_upper - f_lower, h cells",
        arr_e(&palace_split_rel)
    );
    let _ = writeln!(
        t,
        "h_fit_k = {:.6}  # Palace h cells, two-node Maxwell with a, f_r, k free and l_s_nh fixed at [fit.maxwell2_all]",
        palace_h_fit.theta[2]
    );
    let _ = writeln!(t, "h_fit_k_stderr = {:.3e}", palace_h_fit.stderr[2]);
    let _ = writeln!(t, "h_fit_rms_residual_mhz = {:.4}", palace_h_fit.rms_mhz);
    let _ = writeln!(t, "h_fit_k_over_geode = {:.6}", palace_h_fit.theta[2] / k_h);
    let _ = writeln!(
        t,
        "ams_control_cells = [\"{}\", \"{}\"]",
        PALACE_AMS_CONTROL.0, PALACE_AMS_CONTROL.1
    );
    let _ = writeln!(
        t,
        "ams_control_max_rel_diff = {ams_rel:.3e}  # AMS-GMRES vs SuperLU on h at 6.60 nH, both branches\n"
    );
    for (name, level, l, f, p, g, rel) in &palace_rows {
        let _ = writeln!(t, "[[palace.cell]]");
        let _ = writeln!(t, "name = \"{name}\"");
        let _ = writeln!(t, "mesh = \"{}\"", if *level == 0 { "h" } else { "h/2" });
        let _ = writeln!(t, "l_j_nh = {l}");
        let _ = writeln!(t, "palace_f_ghz = {}", arr(f, 9));
        let _ = writeln!(t, "palace_port_epr = {}", arr_e(p));
        let _ = writeln!(t, "geode_f_ghz = {}", arr(&g[..2], 9));
        let _ = writeln!(t, "rel_diff = {}\n", arr_e(rel));
    }
    if let Some(f) = &h2_fit {
        t.push_str("[palace_h2]\n");
        t.push_str("note = \"Palace L_J scan on one uniform gmsh refinement of the fixture (h/2), AMS-preconditioned GMRES; two-node Maxwell fit as [fit.maxwell2_all]\"\n");
        let _ = writeln!(
            t,
            "l_j_nh = {}",
            arr(&h2_pts.iter().map(|p| p.l_nh).collect::<Vec<_>>(), 3)
        );
        let _ = writeln!(
            t,
            "f_lower_ghz = {}",
            arr(&h2_pts.iter().map(|p| p.f[0]).collect::<Vec<_>>(), 9)
        );
        let _ = writeln!(
            t,
            "f_upper_ghz = {}",
            arr(&h2_pts.iter().map(|p| p.f[1]).collect::<Vec<_>>(), 9)
        );
        let _ = writeln!(
            t,
            "port_epr_lower = {}",
            arr(&h2_pts.iter().map(|p| p.p[0]).collect::<Vec<_>>(), 6)
        );
        for (i, name) in f.model.params().iter().enumerate() {
            let _ = writeln!(t, "{name} = {:.9}", f.theta[i]);
            let _ = writeln!(t, "{name}_stderr = {:.3e}", f.stderr[i]);
        }
        let _ = writeln!(t, "g_crossing_mhz = {:.4}", crossing_g_ghz(f) * 1e3);
        let _ = writeln!(t, "rms_residual_mhz = {:.4}", f.rms_mhz);
        let _ = writeln!(t, "max_abs_residual_mhz = {:.4}", f.max_abs_mhz);
        let _ = writeln!(t, "k_ratio_h2_over_h = {:.6}\n", f.theta[2] / k_h);
    }

    // Series loop inductance: the qubit mode's inductive energy outside the
    // junction. With L_s in series, the junction holds L_J/(L_J + L_s) of
    // the qubit node's inductive energy, so L_s = L_J (1 - sum of p).
    let ls_from_p: Vec<f64> = pts
        .iter()
        .map(|p| p.l_nh * (1.0 - p.p.iter().sum::<f64>()))
        .collect();
    let mut sorted = ls_from_p.clone();
    sorted.sort_by(f64::total_cmp);
    let ls_p_median = sorted[sorted.len() / 2];
    let ls_fit = primary.theta[3];
    let base_l = JUNCTION_INDUCTANCE_H * 1e9;
    let offset_pred = 1.0 - (base_l / (base_l + ls_fit)).sqrt();
    let ls_h2_from_epr: Vec<f64> = h2_pts
        .iter()
        .map(|p| p.l_nh * (1.0 - p.p.iter().map(|x| x.abs()).sum::<f64>()))
        .collect();
    t.push_str("[loop_inductance]\n");
    t.push_str("note = \"A by-product of the sweep, NOT applied to any anchor. The qubit mode keeps part of its inductive energy outside the junction; the participations scale as a series inductance L_s, so the linear qubit frequency is a / sqrt(L_J + L_s), not a / sqrt(L_J). Without L_s the two-node fit over 4-30 nH leaves residuals of about 25 MHz; with it they drop to the values in [fit.*]. Reading 4.14 GHz through L_J alone (TransmonAnchor) ignores L_s, which would put the C_Sigma target lower by about C_total L_s / (L_J + L_s): an inference from this mesh, with L_s not converged (the h/2 Palace port EPRs give a larger value). Follow-up #1079; not decided here.\"\n");
    let _ = writeln!(t, "l_s_fit_nh = {ls_fit:.6}  # [fit.maxwell2_all].l_s_nh");
    let _ = writeln!(
        t,
        "l_s_from_participation_nh = {}  # L_J (1 - p_lower - p_upper - p_third), per sweep point",
        arr(&ls_from_p, 5)
    );
    let _ = writeln!(t, "l_s_from_participation_median_nh = {ls_p_median:.5}");
    if !ls_h2_from_epr.is_empty() {
        let _ = writeln!(
            t,
            "l_s_h2_from_palace_epr_nh = {}  # L_J (1 - sum |p[1]|) over the three Palace modes, h/2 cells",
            arr(&ls_h2_from_epr, 5)
        );
    }
    let _ = writeln!(
        t,
        "predicted_downshift_at_14p86_nh = {offset_pred:.6}  # 1 - sqrt(L_J / (L_J + L_s)) with the fitted L_s"
    );
    t.push_str("issue_1043_palace_vs_lumped_offset = 0.00280  # benchmarks/transmon_eigen/results.toml [oracles.palace_target2].qubit_palace_vs_lumped_pct / 100 (sign dropped)\n");
    let anchor_ls =
        TransmonAnchor::devicelayout().c_total_farad * 1e15 * ls_fit / (base_l + ls_fit);
    let _ = writeln!(
        t,
        "implied_anchor_c_sigma_shift_ff = {anchor_ls:.4}  # C_total L_s / (L_J + L_s) at the dressed anchor, with this mesh's L_s; not applied\n"
    );

    t.push_str("[static_proxy]\n");
    t.push_str("method = \"tensor-eps P1 Maxwell matrix, AMG-PCG, uniform red refinement (as benchmarks/transmon_diffopt/c_sigma_convergence.toml [tensor_eps]); conductors island, feedline (floating, reduced out), claw and trace; the resonator is claw + trace\"\n");
    t.push_str("resonator_electrode = \"the ground triangles reached by an edge-adjacency flood fill from the trace's straight section along the feedline, stopped at the shorted end (x = -200 um, 2229 < y < 2251 um); the two cut nodes there stay ground. At each refinement every region is lifted and a shared node goes to the first region in the order ground, island, feedline, claw, trace\"\n");
    let _ = writeln!(t, "trace_triangles = {}", split.n_trace_triangles);
    let _ = writeln!(t, "trace_nodes = {}", split.n_trace_nodes);
    let _ = writeln!(
        t,
        "trace_area_um2 = {:.0}  # about 5000 um x 10 um of line plus the claw",
        split.trace_area_um2
    );
    let _ = writeln!(t, "trace_bbox_um = {}", arr(&split.bbox_um, 1));
    let _ = writeln!(
        t,
        "cut_nodes_um = [{}]",
        split
            .cut_nodes
            .iter()
            .map(|p| format!("[{:.1}, {:.1}]", p[0], p[1]))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "claw_y_max_um = {CLAW_Y_UM}");
    t.push_str("k_static_definition = \"C_g / sqrt((C_Sigma + C_J) C_rr / 2): a lambda/4 line near its fundamental is a parallel LC with half the line capacitance\"\n");
    let lv = |f: &dyn Fn(&StaticLevel) -> f64| st.iter().map(f).collect::<Vec<_>>();
    let _ = writeln!(
        t,
        "n_tets = [{}]",
        st.iter()
            .map(|s| s.n_tets.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(
        t,
        "c_sigma_ff = {}  # h, h/2, h/4; reproduces c_sigma_convergence.toml [tensor_eps]",
        arr(&lv(&|s| s.c_sigma), 6)
    );
    let _ = writeln!(t, "c_g_ff = {}", arr(&lv(&|s| s.c_g), 6));
    let _ = writeln!(t, "c_g_claw_only_ff = {}", arr(&lv(&|s| s.c_g_claw), 6));
    let _ = writeln!(t, "c_rr_ff = {}", arr(&lv(&|s| s.c_rr), 4));
    let _ = writeln!(t, "beta = {}  # C_g / C_Sigma", arr(&betas, 6));
    let _ = writeln!(t, "k_static = {}", arr(&ks, 6));
    let _ = writeln!(
        t,
        "pcg_iterations = [{}]",
        st.iter()
            .map(|s| format!("{:?}", s.pcg_iterations))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(t, "solve_s = {}", arr(&lv(&|s| s.solve_s), 1));
    let _ = writeln!(t, "beta_observed_order = {beta_order:.4}");
    let _ = writeln!(
        t,
        "beta_extrapolated = [{:.6}, {:.6}]  # order-1 and geometric-tail limits; estimates",
        beta_p1, beta_geo
    );
    let _ = writeln!(t, "k_static_observed_order = {ks_order:.4}");
    let _ = writeln!(
        t,
        "k_static_extrapolated = [{:.6}, {:.6}]  # order-1 and geometric-tail limits; estimates",
        ks_p1, ks_geo
    );
    let _ = writeln!(
        t,
        "k_eigen_over_k_static_h = {proxy_ratio_h:.6}  # [fit.maxwell2_all].k / k_static[0]\n"
    );

    t.push_str("[anchor]\n");
    t.push_str("dressed_pair_ghz = [4.14, 5.591]  # DeviceLayout SingleTransmon, Palace linear eigenmodes\n");
    let _ = writeln!(
        t,
        "dressed_c_sigma_ff = {dressed_c:.6}  # TransmonAnchor::devicelayout (kept)"
    );
    t.push_str("inversion = \"Maxwell form, exact for the two-node circuit: f_q^2 + f_r^2 = (1 - k^2)(f_1^2 + f_2^2), f_q^2 f_r^2 = (1 - k^2) f_1^2 f_2^2, bare qubit the lower root (quantum::transmon::coupled_lc_bare_pair_hz); then the linear-LC inversion of TransmonAnchor\"\n");
    let _ = writeln!(
        t,
        "k = {DEVICELAYOUT_COUPLING_K}  # quantum::transmon::DEVICELAYOUT_COUPLING_K"
    );
    let _ = writeln!(
        t,
        "k_band = [{k_lo}, {k_hi}]  # DEVICELAYOUT_COUPLING_K_BAND"
    );
    let _ = writeln!(t, "k_h = {k_h:.6}  # geode, this mesh ([fit.maxwell2_all])");
    let _ = writeln!(
        t,
        "k_h2 = {k_h2:.6}  # Palace, one uniform refinement ([palace_h2])"
    );
    let _ = writeln!(t, "k_eigen_over_k_static_h2 = {ratio_h2:.6}");
    let _ = writeln!(
        t,
        "k_converged_estimates = {}  # Richardson of k_h, k_h2 at the static observed order and at order 1; static limits (order 1, geometric) times k_eigen_over_k_static_h2",
        arr(&estimates, 6)
    );
    t.push_str("k_rule = \"k = midpoint of k_converged_estimates (3 s.f.); band = [k_h2 floored, static geometric-tail limit ceiled] (3 s.f.). The converged k is an extrapolation, not a bound: k rises under refinement at every measured level (eigen h -> h/2, static h -> h/2 -> h/4)\"\n");
    t.push_str("transfer_assumption = \"one k describes DeviceLayout's run. It was measured on geode's meshes (and Palace on the same meshes), not DeviceLayout's; the band takes the converged k, the physical coupling of this geometry. If DeviceLayout's 4.14 GHz came from a mesh as coarse as the fixture, same_mesh_shift_ff applies instead\"\n");
    let _ = writeln!(t, "bare_f_q_ghz = {:.6}", central.0 / 1e9);
    let _ = writeln!(
        t,
        "bare_f_q_band_ghz = [{:.6}, {:.6}]",
        floor_dp(lo.0 / 1e9, 6),
        ceil_dp(fq_odd_hi, 6)
    );
    let _ = writeln!(t, "bare_f_r_ghz = {:.6}", central.1 / 1e9);
    let _ = writeln!(
        t,
        "bare_c_sigma_ff = {:.6}  # TransmonAnchor::devicelayout_bare",
        central.2
    );
    let _ = writeln!(
        t,
        "bare_c_sigma_band_ff = [{:.4}, {:.4}]  # outward-rounded",
        floor_dp(target_band.0, 4),
        ceil_dp(target_band.1, 4)
    );
    let _ = writeln!(t, "shift_ff = {:.6}  # dressed minus bare", central.3);
    let _ = writeln!(
        t,
        "shift_band_ff = [{:.4}, {:.4}]  # dressed minus bare_c_sigma_band_ff",
        floor_dp(dressed_c - target_band.1, 4),
        ceil_dp(dressed_c - target_band.0, 4)
    );
    let _ = writeln!(
        t,
        "same_mesh_k = {k_h:.6}  # [fit.maxwell2_all].k, this mesh"
    );
    let _ = writeln!(t, "same_mesh_bare_c_sigma_ff = {:.6}", same_mesh.2);
    let _ = writeln!(t, "same_mesh_shift_ff = {:.6}", same_mesh.3);
    let _ = writeln!(t, "h2_mesh_bare_c_sigma_ff = {:.6}  # at k_h2", h2_mesh.2);
    let _ = writeln!(t, "h2_mesh_shift_ff = {:.6}", h2_mesh.3);
    let _ = writeln!(
        t,
        "rwa_recipe_g_mhz = {:.4}  # the issue's recipe with this mesh's crossing g as an absolute frequency",
        g_x_hz / 1e6
    );
    let _ = writeln!(t, "rwa_recipe_bare_f_q_ghz = {:.6}", rwa_abs / 1e9);
    let _ = writeln!(
        t,
        "rwa_recipe_bare_c_sigma_ff = {rwa_abs_c:.6}  # Hamiltonian-form bare: not the quantity C_Sigma describes"
    );
    let _ = writeln!(
        t,
        "three_mode_k3_over_k = {k3_over_k:.6}  # [fit.maxwell3_all]"
    );
    let _ = writeln!(t, "three_mode_f3_over_fr = {r3:.6}");
    let _ = writeln!(
        t,
        "three_mode_bare_c_sigma_ff = {c3:.6}  # with the measured 3 lambda/4 node, central k"
    );
    let _ = writeln!(t, "three_mode_minus_two_mode_ff = {:.6}", c3 - central.2);
    let _ = writeln!(
        t,
        "odd_harmonics_bare_c_sigma_ff = {c_odd:.6}  # 3 to 15 lambda/4, each with the measured ratios (inference), central k"
    );
    let _ = writeln!(
        t,
        "odd_harmonics_minus_two_mode_ff = {:.6}",
        c_odd - central.2
    );
    let _ = writeln!(
        t,
        "odd_harmonics_bare_c_sigma_at_k_high_ff = {c_odd_hi:.6}  # the lower end of bare_c_sigma_band_ff"
    );
    let hamiltonian_central = central.0 / (1.0 - DEVICELAYOUT_COUPLING_K.powi(2)).sqrt();
    let _ = writeln!(
        t,
        "hamiltonian_bare_f_q_ghz = {:.6}  # f_q / sqrt(1 - k^2) at the central k",
        hamiltonian_central / 1e9
    );
    t.push('\n');

    t.push_str("[verdicts]\n");
    t.push_str("brackets_source = \"#1077 (#1075): benchmarks/transmon_diffopt/c_sigma_bracket_anchor_93p95.toml, unrounded family bounds\"\n");
    for (name, b) in [
        ("design_1066", BRACKET_1066_DESIGN_FF),
        ("resolution_corrected", BRACKET_CORRECTED_FF),
    ] {
        let _ = writeln!(t, "{name}_bracket_ff = [{:.6}, {:.6}]", b.0, b.1);
        let _ = writeln!(t, "{name}_vs_bare_target = \"{}\"", verdict(b));
        let _ = writeln!(
            t,
            "{name}_minus_bare_central_ff = [{:.4}, {:.4}]  # outward-rounded",
            floor_dp(b.0 - central.2, 4),
            ceil_dp(b.1 - central.2, 4)
        );
        let _ = writeln!(
            t,
            "{name}_minus_bare_band_ff = [{:.4}, {:.4}]  # outward-rounded, against the whole target band",
            floor_dp(b.0 - target_band.1, 4),
            ceil_dp(b.1 - target_band.0, 4)
        );
    }
    let _ = writeln!(
        t,
        "contains_bare_threshold_shift_ff = {:.6}  # dressed anchor minus the corrected design's unrounded lower bound",
        dressed_c - BRACKET_CORRECTED_FF.0
    );

    fs::create_dir_all(&out_dir).expect("output dir");
    let path = out_dir.join("results.toml");
    fs::write(&path, t).expect("write results");
    println!(
        "wrote {} ({:.1}s)",
        path.display(),
        t_all.elapsed().as_secs_f64()
    );
}
