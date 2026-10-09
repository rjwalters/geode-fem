//! Shared fixture, overlap scoring and analytic reference for the
//! Silver-Müller self-consistent tests (`silvermuller_self_consistent.rs`
//! and `silvermuller_self_consistent_vector_tracking.rs`, issue #940).
//!
//! Pulled in with
//! `#[path = "common/silvermuller_sphere.rs"] mod sm_sphere;`.
//!
//! # The analytic reference (issue #940)
//!
//! The fixture is a dielectric sphere (`n = 1.5`, radius `R_s = 1`) in
//! vacuum, truncated at `R_b = 2` by the first-order Silver-Müller
//! condition `n × ∇ × E = j k₀ E_t` (the pencil `(K + j k₀ S, M)`). The
//! same boundary-value problem in the continuum separates in spherical
//! coordinates. [`sm_characteristic`] is its characteristic function for
//! the lowest branch, the `l = 1` electric dipole (`∇ × E` tangential;
//! the `a₁` pole of Bohren and Huffman, the field of
//! [`characteristic_te_open`], whose catalog
//! [`open_space_wgm_roots_n15`] labels it `TE_1,1`). Its roots are the
//! exact eigenvalues that the FEM pencil approximates. Four points on that
//! one branch:
//!
//! | Boundary at `R_b = 2` | `k` | `Q` |
//! |---|---|---|
//! | PEC wall, `E_t = 0` (the limit `k₀ → ∞` of the term `j k₀ S`) | 1.187095 | ∞ |
//! | Silver-Müller at a fixed `k₀ = 20` (not self-consistent) | 1.186600 + 0.032822j | 18.08 |
//! | Silver-Müller, self-consistent `k₀ = Re k` ([`sm_self_consistent_k`]) | 0.557836 + 0.427141j | 0.653 |
//! | Exact outgoing condition: the sphere's open-space Mie pole | 1.258960 + 0.870213j | 0.723 |
//!
//! - The `Q ≈ 18` mode near `k ≈ 1.19` is the **near-PEC** value at
//!   `k₀ = 20`: the term `j k₀ S` is then so large that it nearly pins
//!   `E_t = 0` and the box is almost a closed cavity. It is not a
//!   self-consistent resonance. On the FEM pencil it is
//!   `λ = 1.417260 + 0.078003j` (`SEED_K20_FIRST`), 0.7 % from the
//!   analytic `λ = 1.406943 + 0.077894j`.
//! - The **self-consistent** resonance of this fixture is
//!   `k* = 0.557836 + 0.427141j` (`Q = 0.653`). It is the only fixed point
//!   of the map `k₀ ↦ Re k(k₀)` on this branch for `k₀` from 0.3 to 20, and
//!   the map's slope there is 0.588, so each undamped step of the
//!   iteration shrinks the error by about 0.59.
//! - Blending the Robin coefficient of the boundary condition from
//!   first-order Silver-Müller (`t = 0`) to the exact outgoing condition
//!   (`t = 1`) carries `k*` continuously to the open-space Mie pole (the
//!   catalog's `1.258960 − 0.870213j`, conjugated to this sign
//!   convention). So `k*` is this fixture's version of the sphere's lowest
//!   resonance. The gap between the two is the error of a first-order
//!   absorbing condition at `k R_b ≈ 1.1`, not a discretization error.
//!
//! `analytic_sm_resonance_branch`
//! (`silvermuller_self_consistent_vector_tracking.rs`) computes every
//! number in the table in CI.
#![allow(dead_code)] // each test binary uses a subset

use burn::tensor::backend::BackendTypes;

#[allow(unused_imports)] // referenced by the module docs
use geode_core::analytic::mie::{characteristic_te_open, open_space_wgm_roots_n15};
use geode_core::analytic::mie::{psi_c, psi_prime_c, xi_c, xi_prime_c};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_epsilon, build_epsilon_r, sphere_n_interior_nodes,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::assembly::surface::assemble_silver_muller_surface;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{EigenError, burn_matrix_to_faer};
use geode_core::eigen::self_consistent::SelfConsistentEigensolver;
use geode_core::mesh::{PHYS_OUTER_BOUNDARY, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Refractive index of the fixture's sphere.
pub const N_SPHERE: f64 = 1.5;

/// `λ = k²` as a complex number, for printing and comparing.
pub fn seed(z: (f64, f64)) -> faer::c64 {
    faer::c64::new(z.0, z.1)
}

/// The index of the first eigenvalue past the null cluster (first
/// `|λ| > 1e-3 · max |λ|` in the `|Re λ|`-sorted list): the rule the dense
/// tier uses for its frozen target.
///
/// It names a resonance only by accident. At `k₀ = 1` it is an
/// overdamped mode of the Silver-Müller term (`λ ≈ 0.125 + 2.297j`); at
/// `k₀ = 20` it is the `l = 1` resonance, at its near-PEC value (module
/// docs).
pub fn first_physical_index(lambdas: &[faer::c64]) -> usize {
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one physical mode")
}

/// `(K, S, M, n_eigs, first_physical_idx)` for the sphere fixture at a seed
/// `k₀`: [`build_sphere_matrices`] plus one dense solve at the seed for
/// [`first_physical_index`].
pub fn build_sphere_system(
    seed_k0: f64,
) -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>, usize, usize) {
    let (k_full, s_full, m_full, n_eigs) = build_sphere_matrices();
    let lambdas = FaerComplexEigensolver
        .smallest_complex_eigenvalues(
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            seed_k0,
            n_eigs,
        )
        .expect("initial sphere solve");
    let first_physical = first_physical_index(&lambdas);
    (k_full, s_full, m_full, n_eigs, first_physical)
}

/// The sphere fixture's `(K, S, M)` and the dense tier's `n_eigs` (twice
/// the gradient null-space dimension). No eigensolve.
pub fn build_sphere_matrices() -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>, usize) {
    let f = read_sphere_fixture().expect("fixture load");
    let epsilon_r = build_epsilon_r(&f.tet_physical_tags, N_SPHERE);

    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &epsilon_r,
    );
    let s_full = assemble_silver_muller_surface(
        &f.mesh,
        &f.boundary_triangles,
        &f.triangle_physical_tags,
        PHYS_OUTER_BOUNDARY,
        &edges,
    );
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);

    let spurious_lower_bound = sphere_n_interior_nodes(&f.mesh, geode_core::mesh::R_BUFFER);
    let n_eigs = (spurious_lower_bound * 2).max(20);

    (k_full, s_full, m_full, n_eigs)
}

/// Q with the standard outgoing-wave convention.
pub fn q_of(k: faer::c64) -> f64 {
    if k.im.abs() < 1e-12 {
        f64::INFINITY
    } else {
        k.re / (2.0 * k.im.abs())
    }
}

// ---------------------------------------------------------------------
// Recording and overlap scoring (issue #917).
// ---------------------------------------------------------------------

/// One recorded eigensolve of a self-consistent run.
pub struct Solve {
    pub k0: f64,
    pub sigma: faer::c64,
    pub pairs: Vec<(faer::c64, Vec<faer::c64>)>,
}

/// Wraps an eigensolver and records every solve the driver makes, with
/// eigenvectors, so a test can score what the driver picked. It changes
/// nothing about the run: the driver gets the inner solver's eigenvalues.
pub struct Recording<'a, S: ?Sized> {
    inner: &'a S,
    pub solves: std::cell::RefCell<Vec<Solve>>,
}

impl<'a, S: SelfConsistentEigensolver + ?Sized> Recording<'a, S> {
    pub fn new(inner: &'a S) -> Self {
        Self {
            inner,
            solves: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl<S: SelfConsistentEigensolver + ?Sized> SelfConsistentEigensolver for Recording<'_, S> {
    fn pencil_eigenvalues(
        &self,
        k: faer::MatRef<f64>,
        s: faer::MatRef<f64>,
        m: faer::MatRef<f64>,
        k0: f64,
        sigma: faer::c64,
        n: usize,
    ) -> Result<Vec<faer::c64>, EigenError> {
        Ok(self
            .pencil_eigenpairs(k, s, m, k0, sigma, n)?
            .into_iter()
            .map(|p| p.0)
            .collect())
    }

    fn pencil_eigenpairs(
        &self,
        k: faer::MatRef<f64>,
        s: faer::MatRef<f64>,
        m: faer::MatRef<f64>,
        k0: f64,
        sigma: faer::c64,
        n: usize,
    ) -> Result<Vec<(faer::c64, Vec<faer::c64>)>, EigenError> {
        let pairs = self.inner.pencil_eigenpairs(k, s, m, k0, sigma, n)?;
        self.solves.borrow_mut().push(Solve {
            k0,
            sigma,
            pairs: pairs.clone(),
        });
        Ok(pairs)
    }
}

/// One step of a recorded run: what was picked, and how it relates to the
/// previous pick.
#[derive(Debug, Clone, Copy)]
pub struct TraceStep {
    pub k0: f64,
    /// The picked eigenvalue.
    pub lambda: faer::c64,
    /// Bilinear M-overlap of the pick with the previous pick (`None` at the
    /// first solve).
    pub overlap: Option<f64>,
    /// The largest overlap with the previous pick among the candidates that
    /// were **not** picked (`None` at the first solve or with one candidate).
    pub best_unpicked: Option<f64>,
}

/// The nonzeros of a dense matrix, so the overlaps below cost `O(nnz)`.
pub fn nonzeros(m: faer::MatRef<f64>) -> Vec<(usize, usize, f64)> {
    let mut out = Vec::new();
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            let v = m[(i, j)];
            if v != 0.0 {
                out.push((i, j, v));
            }
        }
    }
    out
}

/// Bilinear form `uᵀ M v` (no conjugation), the inner product of the
/// complex-symmetric pencil.
pub fn m_bilinear(m: &[(usize, usize, f64)], u: &[faer::c64], v: &[faer::c64]) -> faer::c64 {
    let mut acc = faer::c64::new(0.0, 0.0);
    for &(i, j, mij) in m {
        acc += u[i] * v[j] * mij;
    }
    acc
}

/// `|uᵀ M v| / √(|uᵀ M u| · |vᵀ M v|)`: the score
/// `self_consistent_k_vector_tracked` gives a candidate `v` against the
/// previous target `u`. 0 for two different eigenvectors of one pencil (they
/// are M-orthogonal), 1 for the same eigenvector. Every factor is a modulus,
/// so the score does not depend on the phase or scale either solver returns
/// an eigenvector with (issue #988): the dense solver's arbitrary phases
/// used to inflate it, to 1.05 to 5.98 for one eigenvector in
/// `vector_tracked_beats_frozen_int_idx`, when it divided by `|Re|`.
pub fn m_overlap(m: &[(usize, usize, f64)], u: &[faer::c64], v: &[faer::c64]) -> f64 {
    let uv = m_bilinear(m, u, v);
    let uu = m_bilinear(m, u, u).norm();
    let vv = m_bilinear(m, v, v).norm();
    uv.norm() / (uu * vv).sqrt().max(1e-300)
}

/// Score a recorded run. `pick` returns the index the driver's target rule
/// selected in a solve, given the previous pick's eigenvector (`None` at the
/// first solve).
pub fn overlap_trace(
    solves: &[Solve],
    m: faer::MatRef<f64>,
    pick: impl Fn(&Solve, Option<&[faer::c64]>) -> usize,
) -> Vec<TraceStep> {
    let m = nonzeros(m);
    let mut prev: Option<&Vec<faer::c64>> = None;
    let mut out = Vec::new();
    for solve in solves {
        let picked = pick(solve, prev.map(|v| v.as_slice()));
        let (overlap, best_unpicked) = match prev {
            None => (None, None),
            Some(u) => {
                let scores: Vec<f64> = solve
                    .pairs
                    .iter()
                    .map(|(_, v)| m_overlap(&m, u, v))
                    .collect();
                let best_unpicked = scores
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != picked)
                    .map(|(_, s)| *s)
                    .fold(None, |acc: Option<f64>, s| {
                        Some(acc.map_or(s, |a| a.max(s)))
                    });
                (Some(scores[picked]), best_unpicked)
            }
        };
        out.push(TraceStep {
            k0: solve.k0,
            lambda: solve.pairs[picked].0,
            overlap,
            best_unpicked,
        });
        prev = Some(&solve.pairs[picked].1);
    }
    out
}

/// Solve calls run with the damping factor 0.5 before the drivers switch
/// to full steps (`DAMPED_ITERATIONS` in `eigen::self_consistent`).
const DAMPED_SOLVES: usize = 3;

/// Check a reconstructed trace against what the driver did: each recorded
/// `k₀` must be the damped update `k₀ + α (Re √λ − k₀)` from the previous
/// `k₀` and the pick the trace claims for it (`α = 0.5` for the first
/// [`DAMPED_SOLVES`] solves, then 1). A trace whose pick rule is not the
/// driver's fails here at the first step where the two differ.
pub fn assert_trace_drives_k0(trace: &[TraceStep], label: &str) {
    for i in 1..trace.len() {
        let prev = trace[i - 1];
        let alpha = if i <= DAMPED_SOLVES { 0.5 } else { 1.0 };
        let k = geode_core::eigen::wavenumber::principal_sqrt(prev.lambda);
        let expected = prev.k0 + alpha * (k.re - prev.k0);
        assert!(
            (trace[i].k0 - expected).abs() <= 1e-12 * expected.abs().max(1.0),
            "{label}: solve {} ran at k0 = {}, but the pick the trace claims at solve {i} \
             (λ = {}) gives k0 = {expected}: the driver picked another eigenvalue",
            i + 1,
            trace[i].k0,
            prev.lambda,
        );
    }
}

/// The index of the eigenvalue nearest `anchor` in a solve.
fn nearest_to(solve: &Solve, anchor: faer::c64) -> usize {
    solve
        .pairs
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1.0 - anchor).norm().total_cmp(&(b.1.0 - anchor).norm()))
        .map(|(i, _)| i)
        .expect("non-empty solve")
}

/// Print a trace, one line per solve.
pub fn print_trace(trace: &[TraceStep], solves: &[Solve], label: &str) {
    for (i, step) in trace.iter().enumerate() {
        eprintln!(
            "[{label}] it {:2} k0 = {:.6} n = {:3} pick λ = {:.6}{:+.6}i overlap = {} \
             best unpicked = {}",
            i + 1,
            step.k0,
            solves[i].pairs.len(),
            step.lambda.re,
            step.lambda.im,
            step.overlap.map_or("n/a".into(), |o| format!("{o:.4}")),
            step.best_unpicked
                .map_or("n/a".into(), |o| format!("{o:.4}")),
        );
    }
}

/// Score a recorded `ModeTarget::Nearest` run of `self_consistent_k_with`.
///
/// The pick is reconstructed from the record, not re-derived: under
/// `Nearest` the driver passes its anchor (the seed, then the previous
/// target) as the shift `σ` and picks the eigenvalue nearest it. The
/// function checks that reading against the record: every `σ` after the
/// first must be the previous solve's pick, bit for bit.
pub fn proximity_trace(solves: &[Solve], m: faer::MatRef<f64>, label: &str) -> Vec<TraceStep> {
    let trace = overlap_trace(solves, m, |solve, _| nearest_to(solve, solve.sigma));
    for i in 1..trace.len() {
        assert_eq!(
            (solves[i].sigma.re, solves[i].sigma.im),
            (trace[i - 1].lambda.re, trace[i - 1].lambda.im),
            "{label}: solve {} was not shifted at the previous pick",
            i + 1
        );
    }
    print_trace(&trace, solves, label);
    assert_trace_drives_k0(&trace, label);
    trace
}

/// Score a recorded `ModeTarget::Index(idx)` run of `self_consistent_k_with`
/// on a full-spectrum solver: the pick is entry `idx` of every solve.
pub fn index_trace(
    solves: &[Solve],
    m: faer::MatRef<f64>,
    idx: usize,
    label: &str,
) -> Vec<TraceStep> {
    let trace = overlap_trace(solves, m, |_, _| idx);
    print_trace(&trace, solves, label);
    assert_trace_drives_k0(&trace, label);
    trace
}

/// Score a recorded run of `self_consistent_k_vector_tracked_with`: the
/// first pick is `first` (the seed rule), every later pick is the candidate
/// of largest M-overlap with the previous pick, as in the driver.
pub fn tracked_trace(
    solves: &[Solve],
    m: faer::MatRef<f64>,
    first: impl Fn(&Solve) -> usize,
    label: &str,
) -> Vec<TraceStep> {
    let m_nz = nonzeros(m);
    let trace = overlap_trace(solves, m, |solve, prev| match prev {
        None => first(solve),
        Some(u) => solve
            .pairs
            .iter()
            .enumerate()
            .max_by(|a, b| m_overlap(&m_nz, u, &a.1.1).total_cmp(&m_overlap(&m_nz, u, &b.1.1)))
            .map(|(i, _)| i)
            .expect("non-empty solve"),
    });
    print_trace(&trace, solves, label);
    assert_trace_drives_k0(&trace, label);
    trace
}

/// The index of the eigenvalue nearest the shift `σ` the solve was made
/// at: the seed pick of a `ModeTarget::Nearest` run.
pub fn nearest_to_sigma(solve: &Solve) -> usize {
    nearest_to(solve, solve.sigma)
}

/// The smallest overlap between successive picks over a whole trace.
pub fn min_overlap(trace: &[TraceStep]) -> f64 {
    trace
        .iter()
        .filter_map(|s| s.overlap)
        .fold(f64::INFINITY, f64::min)
}

// ---------------------------------------------------------------------
// Analytic reference (issue #940): module docs.
// ---------------------------------------------------------------------

/// Sphere radius and Silver-Müller boundary radius of the fixture.
pub const R_S: f64 = geode_core::mesh::R_SPHERE;
pub const R_B: f64 = geode_core::mesh::R_BUFFER;

/// The boundary condition at `r = R_b` in [`sm_characteristic`].
#[derive(Debug, Clone, Copy)]
pub enum OuterBc {
    /// Perfect electric conductor, `E_t = 0`.
    Pec,
    /// First-order Silver-Müller at a fixed `k₀`: `n × ∇ × E = j k₀ E_t`.
    SilverMuller(f64),
    /// Silver-Müller at `k₀ = Re k` (the self-consistent condition; the
    /// function is then not analytic in `k`).
    SilverMullerSelfConsistent,
    /// Blend `(1 − t)·(self-consistent Silver-Müller) + t·(exact outgoing)`
    /// of the two Robin coefficients, for the homotopy to open space.
    Homotopy(f64),
}

fn c(re: f64) -> faer::c64 {
    faer::c64::new(re, 0.0)
}

/// Characteristic function of the `l = 1` electric-dipole branch of the
/// continuum fixture (module docs): a 3×3 determinant in the amplitudes
/// `(C, A, B)` of `u = C ψ₁(n k r)` inside the sphere and
/// `u = A ψ₁(k r) + B ξ₁(k r)` in the vacuum shell, where `u = r g` and
/// `∇ × E = g X₁ₘ`.
///
/// Rows: `u` continuous and `u′/ε` continuous at `R_s` (the field of
/// [`characteristic_te_open`]), and the outer condition written as
/// `u′ = β u` at `R_b`. For `n × ∇ × E = j k₀ E_t`, `β = −j k² / k₀`. The
/// exact outgoing condition in this sign convention (`Im k > 0` decays;
/// the conjugate of the `exp(−iωt)` convention of the Mie catalog) is
/// `β = k (2ψ₁′ − ξ₁′)/(2ψ₁ − ξ₁)` at `k R_b`, the Riccati-Hankel function
/// of the second kind.
pub fn sm_characteristic(k: faer::c64, bc: OuterBc) -> faer::c64 {
    let l = 1;
    let n = N_SPHERE;
    let x_in = k * c(n * R_S);
    let x_s = k * c(R_S);
    let z = k * c(R_B);
    let beta_sm = |k0: f64| faer::c64::new(0.0, -1.0) * k * k * c(1.0 / k0);
    let beta_open =
        k * (psi_prime_c(l, z) * c(2.0) - xi_prime_c(l, z)) / (psi_c(l, z) * c(2.0) - xi_c(l, z));
    let outer = match bc {
        // `E_t ∝ u′`, so the PEC row is `u′(R_b) = 0`.
        OuterBc::Pec => [c(0.0), psi_prime_c(l, z), xi_prime_c(l, z)],
        _ => {
            let beta = match bc {
                OuterBc::SilverMuller(k0) => beta_sm(k0),
                OuterBc::SilverMullerSelfConsistent => beta_sm(k.re),
                OuterBc::Homotopy(t) => beta_sm(k.re) * c(1.0 - t) + beta_open * c(t),
                OuterBc::Pec => unreachable!(),
            };
            [
                c(0.0),
                k * psi_prime_c(l, z) - beta * psi_c(l, z),
                k * xi_prime_c(l, z) - beta * xi_c(l, z),
            ]
        }
    };
    let m = [
        [psi_c(l, x_in), -psi_c(l, x_s), -xi_c(l, x_s)],
        [
            psi_prime_c(l, x_in) * c(1.0 / n),
            -psi_prime_c(l, x_s),
            -xi_prime_c(l, x_s),
        ],
        outer,
    ];
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// A root of [`sm_characteristic`] near `guess`, by Newton's method on the
/// two real components with a central-difference Jacobian (the
/// self-consistent function is not analytic in `k`). Returns the root and
/// `|f(k)| / |f(k (1 + 10⁻³))|`, which is about `10⁻³` at a point that is
/// not a root and close to round-off at a root.
pub fn sm_root(guess: faer::c64, bc: OuterBc) -> (faer::c64, f64) {
    let f = |k: faer::c64| sm_characteristic(k, bc);
    let mut k = guess;
    for _ in 0..100 {
        let fk = f(k);
        let h = 1e-7 * k.norm().max(1.0);
        let dx = (f(k + c(h)) - f(k - c(h))) * c(0.5 / h);
        let dy = (f(k + faer::c64::new(0.0, h)) - f(k - faer::c64::new(0.0, h))) * c(0.5 / h);
        // Solve [dx.re dy.re; dx.im dy.im] · (δx, δy) = −(f.re, f.im).
        let det = dx.re * dy.im - dy.re * dx.im;
        let ddx = (-fk.re * dy.im + fk.im * dy.re) / det;
        let ddy = (-dx.re * fk.im + dx.im * fk.re) / det;
        k += faer::c64::new(ddx, ddy);
        if ddx.hypot(ddy) < 1e-14 * k.norm() {
            break;
        }
    }
    (k, f(k).norm() / f(k * c(1.0 + 1e-3)).norm().max(1e-300))
}

/// The self-consistent `l = 1` resonance of the continuum fixture,
/// `k* ≈ 0.557836 + 0.427141j` (module docs), with its relative residual.
pub fn sm_self_consistent_k() -> (faer::c64, f64) {
    sm_root(
        faer::c64::new(0.55, 0.42),
        OuterBc::SilverMullerSelfConsistent,
    )
}
