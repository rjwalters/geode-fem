//! Golden 4 of issue #858 (Epic #837 Phase 2), the kernel tripwire, plus
//! the structural checks of the Bloch-phase constraint, its refusals, and
//! the realification cross-check.
//!
//! - **Kernel at `k ≠ 0` (golden 4).** For `k` not in the reciprocal
//!   lattice there are no harmonic fields, and the Bloch-phased reduced
//!   gradient `G_r(k)` is injective. So `dim ker K_r(k) = n_nodes_red`
//!   exactly. That compares with `n_nodes_red + 2` on a 3-torus at Γ, or
//!   at `k = G` in the reciprocal lattice: `n_nodes_red − 1` gradients
//!   plus 3 harmonic fields. With PEC lids it compares with
//!   `n_nodes_red + 1` (`E = ẑ`). It is counted by a dense Hermitian
//!   eigen decomposition of `K_r`, on lexicographic and randomly
//!   renumbered meshes (reversed edge orientations).
//! - **Tripwire.** If the Bloch phase sits on the edges but not on the
//!   nodes, `G P_node ≠ P_edge G_r`. Then `image(G_r)` is not the kernel
//!   (`K_r G_r ≠ 0`), and the deflated solve finds spurious near-zero
//!   modes that the correct one does not.
//! - **Hermitian, not symmetric.** `K_r(k)` is Hermitian to round-off and
//!   far from complex-symmetric, with `K_r(k)ᵀ = K_r(−k)`.
//! - **Realification.** The real `2n` pencil `[[Re, −Im], [Im, Re]]` has
//!   exactly the Hermitian spectrum, doubled. The native block solve
//!   matches the dense spectrum.
//! - **Refusals (#804).** p=2 periodic and the zero-phase cavity solve
//!   with a complex constraint fail loudly. (A driven solve with a complex
//!   phase is accepted since issue #870.)

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;
use faer::linalg::solvers::Solve;
use faer::sparse::SparseColMat;
use faer::{Mat, Side, c64};

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::{PeriodicConstraint, PeriodicError};
use geode_core::driven::periodic::PeriodicDrivenOperator;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator};
use geode_core::eigen::bloch::{BlochCell, BlochError, BlochSettings, realify_hermitian};
use geode_core::eigen::pec_cavity::{PecCavityError, PecCavityMaterials, PecCavitySettings};
use geode_core::eigen::periodic_cavity::solve_periodic_cavity_modes;
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// A generic wave vector (not in the reciprocal lattice of the unit cube).
const K_GENERIC: [f64; 3] = [0.37 * PI, 0.21 * PI, 0.13 * PI];

struct Cell {
    mesh: TetMesh,
    constraint: PeriodicConstraint,
    cell: BlochCell,
}

fn cell(mut mesh: TetMesh, axes: &[usize], pec_lids: bool) -> Cell {
    let pairs = box_periodic_pairs(&mesh, axes);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = pec_lids.then(|| {
        let zmax = mesh.nodes.iter().map(|p| p[2]).fold(0.0, f64::max);
        let lo = boundary_triangles_on_plane(&mesh, 2, 0.0, 1e-9);
        let hi = boundary_triangles_on_plane(&mesh, 2, zmax, 1e-9);
        space.pec_interior_mask(&mesh, &[&lo, &hi]).unwrap()
    });
    let constraint = PeriodicConstraint::build(&space, &map, mask.as_deref()).unwrap();
    let eps = vec![1.0; mesh.n_tets()];
    let cell = BlochCell::new::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &constraint,
        &device(),
    )
    .unwrap();
    Cell {
        mesh,
        constraint,
        cell,
    }
}

fn dense_c(a: &SparseColMat<usize, c64>) -> Mat<c64> {
    let mut d = Mat::<c64>::zeros(a.nrows(), a.ncols());
    for j in 0..a.ncols() {
        for (i, &v) in a.as_ref().row_idx_of_col(j).zip(a.as_ref().val_of_col(j)) {
            d[(i, j)] += v;
        }
    }
    d
}

fn max_abs(a: &Mat<c64>) -> f64 {
    let mut m = 0.0_f64;
    for j in 0..a.ncols() {
        for i in 0..a.nrows() {
            m = m.max(a[(i, j)].norm());
        }
    }
    m
}

/// `dim ker` of a Hermitian PSD matrix by its eigenvalues (relative
/// threshold), with the spectral gap printed.
fn hermitian_kernel(a: &Mat<c64>, label: &str) -> usize {
    let s = a.self_adjoint_eigenvalues(Side::Lower).unwrap();
    let smax = s.iter().map(|x| x.abs()).fold(0.0, f64::max);
    let tol = 1e-9 * smax;
    let kernel = s.iter().filter(|x| x.abs() <= tol).count();
    let below = s
        .iter()
        .map(|x| x.abs())
        .filter(|x| *x <= tol)
        .fold(0.0, f64::max);
    let above = s
        .iter()
        .map(|x| x.abs())
        .filter(|x| *x > tol)
        .fold(f64::INFINITY, f64::min);
    eprintln!("    {label}: kernel {kernel}, gap {below:.3e} | {above:.3e} (max {smax:.3e})");
    kernel
}

fn kernel_dim(c: &Cell, k: [f64; 3], label: &str) -> (usize, usize) {
    let (kr, _) = c.cell.reduced_pencil(k).unwrap();
    let pc = c.cell.phased_constraint(k);
    (
        hermitian_kernel(&dense_c(&kr), label),
        pc.nodes().n_reduced(),
    )
}

/// Golden 4: `dim ker K_r(k) = n_nodes_red` exactly off the reciprocal
/// lattice, on a 3-torus and between PEC lids, lexicographic and
/// renumbered.
#[test]
fn bloch_kernel_dimension_is_exactly_the_phased_gradients() {
    for (label, mesh) in [
        ("lexicographic", box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0])),
        (
            "renumbered",
            renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 858),
        ),
    ] {
        let c = cell(mesh, &[0, 1, 2], false);
        let (kg, nn) = kernel_dim(&c, K_GENERIC, &format!("{label} T³ k generic"));
        assert_eq!(kg, nn, "{label}: dim ker at generic k");
        let (k0, nn0) = kernel_dim(&c, [0.0; 3], &format!("{label} T³ Γ"));
        assert_eq!(k0, nn0 - 1 + 3, "{label}: dim ker at Γ");
        let (kgv, nng) = kernel_dim(&c, [2.0 * PI, 0.0, 0.0], &format!("{label} T³ k = G"));
        assert_eq!(kgv, nng - 1 + 3, "{label}: dim ker at a reciprocal vector");
        // Along a single axis only: still no harmonic field.
        let (kx, nnx) = kernel_dim(&c, [0.5 * PI, 0.0, 0.0], &format!("{label} T³ k ∥ x"));
        assert_eq!(kx, nnx, "{label}: dim ker at k = (π/2, 0, 0)");
    }
    let lids = cell(box_tet_mesh([3, 3, 2], [1.0, 1.0, 0.5]), &[0, 1], true);
    let (kg, nn) = kernel_dim(&lids, K_GENERIC, "lids k generic");
    assert_eq!(kg, nn, "lids: dim ker at generic k");
    let (k0, nn0) = kernel_dim(&lids, [0.0; 3], "lids Γ");
    assert_eq!(k0, nn0 + 1, "lids: dim ker at Γ (E = ẑ)");
}

/// The commuting relation and `K_r G_r = 0` with the Bloch phase, and the
/// edges-only tripwire that breaks both.
#[test]
fn bloch_gradient_commutes_and_edges_only_tripwire_breaks_it() {
    let c = cell(
        renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 7),
        &[0, 1, 2],
        false,
    );
    let pc = c.cell.phased_constraint(K_GENERIC);
    assert!(pc.has_complex_phase());
    let comm = pc.check_gradient_commutes();
    let (kr, _) = c.cell.reduced_pencil(K_GENERIC).unwrap();
    let kd = dense_c(&kr);
    let kgr = max_abs(&(&kd * dense_c(&pc.reduced_gradient_complex()))) / max_abs(&kd);
    eprintln!("phased: ‖G P_node − P_edge G_r‖∞ = {comm:.3e}, ‖K_r G_r‖/‖K_r‖ = {kgr:.3e}");
    assert!(comm < 1e-14, "commutator {comm}");
    assert!(kgr < 1e-12, "K_r G_r = {kgr}");

    let wrong = c.constraint.tripwire_bloch_phase_edges_only(K_GENERIC);
    let comm_w = wrong.check_gradient_commutes();
    let kgr_w = max_abs(&(&kd * dense_c(&wrong.reduced_gradient_complex()))) / max_abs(&kd);
    eprintln!("tripwire (edges only): commutator {comm_w:.3e}, ‖K_r G_r‖/‖K_r‖ = {kgr_w:.3e}");
    assert!(
        comm_w > 0.1,
        "the tripwire must break the commuting relation"
    );
    assert!(kgr_w > 1e-2, "the tripwire gradient must leave the kernel");
}

/// `K_r(k)` is Hermitian, not complex-symmetric, and `K_r(k)ᵀ = K_r(−k)`.
#[test]
fn bloch_pencil_is_hermitian_not_symmetric() {
    let c = cell(box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), &[0, 1, 2], false);
    let (kr, mr) = c.cell.reduced_pencil(K_GENERIC).unwrap();
    let (krm, _) = c.cell.reduced_pencil(K_GENERIC.map(|x| -x)).unwrap();
    for (name, a) in [("K_r", &kr), ("M_r", &mr)] {
        let d = dense_c(a);
        let herm = max_abs(&(&d - d.adjoint())) / max_abs(&d);
        let sym = max_abs(&(&d - d.transpose())) / max_abs(&d);
        eprintln!("{name}: ‖A − Aᴴ‖ = {herm:.3e}, ‖A − Aᵀ‖ = {sym:.3e}");
        assert!(herm < 1e-14, "{name} not Hermitian: {herm}");
        assert!(sym > 1e-2, "{name} unexpectedly complex-symmetric: {sym}");
    }
    let t = max_abs(&(dense_c(&kr).transpose() - dense_c(&krm))) / max_abs(&dense_c(&kr));
    assert!(t < 1e-14, "K_r(k)ᵀ ≠ K_r(−k): {t}");
}

/// Zero phase is the identity, and re-phasing replaces `k` rather than
/// composing. `dphase_dk` matches a finite difference of the coefficients.
#[test]
fn bloch_phase_is_exact_and_not_cumulative() {
    let c = cell(box_tet_mesh([2, 2, 2], [1.0, 1.0, 1.0]), &[0, 1, 2], false);
    let z = c.constraint.with_bloch_phase([0.0; 3]);
    assert_eq!(z.dofs(), c.constraint.dofs());
    assert_eq!(z.nodes(), c.constraint.nodes());
    let twice = c
        .constraint
        .with_bloch_phase([1.0, 2.0, 3.0])
        .with_bloch_phase(K_GENERIC);
    let once = c.constraint.with_bloch_phase(K_GENERIC);
    assert_eq!(twice.dofs(), once.dofs());
    assert_eq!(once.bloch_k(), K_GENERIC);

    let h = 1e-6;
    let d = once.dphase_dk();
    for axis in 0..3 {
        let mut kp = K_GENERIC;
        let mut km = K_GENERIC;
        kp[axis] += h;
        km[axis] -= h;
        let (p, m) = (
            c.constraint.with_bloch_phase(kp),
            c.constraint.with_bloch_phase(km),
        );
        let mut idx = 0;
        let mut worst = 0.0_f64;
        for i in 0..once.n_full() {
            for ((_, cp), (_, cm)) in p.dofs().row(i).zip(m.dofs().row(i)) {
                let fd = (cp - cm) * (0.5 / h);
                worst = worst.max((fd - d[idx][axis]).norm());
                idx += 1;
            }
        }
        assert!(worst < 1e-8, "dphase_dk axis {axis}: {worst}");
    }
}

/// Realification: the real `2n` pencil has the Hermitian spectrum, every
/// eigenvalue doubled. The native block solve matches the dense spectrum.
#[test]
fn realified_spectrum_is_exactly_doubled_and_native_solve_matches() {
    let c = cell(
        renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 11),
        &[0, 1, 2],
        false,
    );
    let (kr, mr) = c.cell.reduced_pencil(K_GENERIC).unwrap();
    let n = kr.nrows();
    // Dense Hermitian generalized spectrum through M⁻¹K (small n).
    let gen_c = |k: &Mat<c64>, m: &Mat<c64>| -> Vec<f64> {
        let x = m.partial_piv_lu().solve(k);
        let mut ev: Vec<f64> = x.eigenvalues().unwrap().iter().map(|z| z.re).collect();
        ev.sort_by(f64::total_cmp);
        ev
    };
    let herm = gen_c(&dense_c(&kr), &dense_c(&mr));
    let kre = realify_hermitian(kr.as_ref()).to_dense();
    let mre = realify_hermitian(mr.as_ref()).to_dense();
    assert_eq!(kre.nrows(), 2 * n);
    let sym = (&kre - kre.transpose()).norm_max() / kre.norm_max();
    assert!(sym < 1e-14, "realified K not symmetric: {sym}");
    let xr = mre.partial_piv_lu().solve(&kre);
    let mut real: Vec<f64> = xr.eigenvalues().unwrap().iter().map(|z| z.re).collect();
    real.sort_by(f64::total_cmp);
    let scale = herm.iter().copied().fold(0.0, f64::max);
    let mut worst = 0.0_f64;
    for (i, &h) in herm.iter().enumerate() {
        worst = worst
            .max((real[2 * i] - h).abs())
            .max((real[2 * i + 1] - h).abs());
    }
    eprintln!(
        "realified vs doubled Hermitian spectrum: max |Δλ| / λ_max = {:.3e}",
        worst / scale
    );
    assert!(
        worst < 1e-9 * scale,
        "realified spectrum not exactly doubled"
    );

    // Native block solve: the lowest physical modes (kernel deflated).
    let want: Vec<f64> = herm
        .iter()
        .copied()
        .filter(|&l| l > 1e-8 * scale)
        .take(10)
        .collect();
    let modes = c.cell.solve(K_GENERIC, &BlochSettings::new(10)).unwrap();
    assert_eq!(
        modes.n_gradient,
        c.cell.phased_constraint(K_GENERIC).nodes().n_reduced()
    );
    let got: Vec<f64> = modes.modes.iter().map(|m| m.lambda).collect();
    eprintln!(
        "dense λ = {want:.6?}\nblock λ = {got:.6?} (basis {})",
        modes.basis_dim
    );
    for (g, w) in got.iter().zip(&want) {
        assert!((g - w).abs() < 1e-9 * w, "{g} vs {w}");
    }
    // Interior target: the two eigenvalues nearest a shift between λ₅ and
    // λ₆ (indefinite K − σM, same Hermitian path).
    let near = c
        .cell
        .solve(
            K_GENERIC,
            &BlochSettings {
                sigma: Some(0.5 * (want[4] + want[5])),
                ..BlochSettings::new(2)
            },
        )
        .unwrap();
    let got_near: Vec<f64> = near.modes.iter().map(|m| m.lambda).collect();
    eprintln!("nearest-σ λ = {got_near:.6?} (want {:.6?})", &want[4..6]);
    for (g, w) in got_near.iter().zip(&want[4..6]) {
        assert!((g - w).abs() < 1e-9 * w, "{g} vs {w}");
    }
    for m in &modes.modes {
        assert!(m.residual_rel < 1e-8);
        assert!(!m.is_static);
        let norm: f64 = {
            let (_, mf) = c.cell.full_pencil();
            let mut acc = c64::new(0.0, 0.0);
            for j in 0..mf.ncols() {
                for (i, &v) in mf.as_ref().row_idx_of_col(j).zip(mf.as_ref().val_of_col(j)) {
                    acc += m.vector[i].conj() * m.vector[j] * v;
                }
            }
            acc.re
        };
        assert!((norm - 1.0).abs() < 1e-9, "eᴴ M e = {norm}");
    }
    let _ = &c.mesh;
}

fn driven_operator(mesh: &TetMesh) -> DrivenOperator {
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let mask = vec![true; mesh.edges().len()];
    DrivenOperator::assemble::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        &CurrentSource::from_centroids(mesh, |_| [c64::new(0.0, 0.0); 3]),
        &device(),
    )
    .unwrap()
}

/// Refusals: p=2 periodic and the zero-phase cavity solve with a complex
/// constraint (complex-phase driven is accepted since issue #870).
#[test]
fn bloch_refusals_are_typed_and_loud() {
    let c = cell(box_tet_mesh([2, 2, 2], [1.0, 1.0, 1.0]), &[0, 1, 2], false);
    let phased = c.constraint.with_bloch_phase(K_GENERIC);
    // Issue #870 (Epic #837 Phase 3a) lifted the complex-phase driven
    // refusal: the Bloch driven solve is direct LU on the non-symmetric
    // `Pᴴ A P` (goldens in `floquet_ports.rs`).
    assert!(PeriodicDrivenOperator::new(driven_operator(&c.mesh), &phased).is_ok());
    // A real (anti-periodic, k·d = π) phase is still a sign: allowed.
    let anti = c.constraint.with_bloch_phase([PI, 0.0, 0.0]);
    assert!(!anti.has_complex_phase());
    assert!(PeriodicDrivenOperator::new(driven_operator(&c.mesh), &anti).is_ok());
    // Zero phase (#839) still allowed.
    assert!(PeriodicDrivenOperator::new(driven_operator(&c.mesh), &c.constraint).is_ok());

    let eps = vec![1.0; c.mesh.n_tets()];
    match solve_periodic_cavity_modes::<B>(
        &c.mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &phased,
        &PecCavitySettings::new(1.0, 1),
        &device(),
    ) {
        Err(PecCavityError::InvalidInput(m)) => assert!(m.contains("Bloch"), "{m}"),
        other => panic!("want InvalidInput, got {:?}", other.err()),
    }

    let mut mesh2 = box_tet_mesh([2, 2, 2], [1.0, 1.0, 1.0]);
    let pairs = box_periodic_pairs(&mesh2, &[0, 1, 2]);
    let map = PeriodicMap::build(&mut mesh2, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let p2 = HcurlSpace::build(&mesh2, ElementOrder::P2);
    assert!(matches!(
        PeriodicConstraint::build(&p2, &map, None),
        Err(PeriodicError::Unsupported { .. })
    ));

    // A cell with no periodic pair has no lattice: refused.
    let mut mesh0 = box_tet_mesh([2, 2, 2], [1.0, 1.0, 1.0]);
    let map0 = PeriodicMap::build(&mut mesh0, &[], &PeriodicMatchOptions::default()).unwrap();
    let s0 = HcurlSpace::build(&mesh0, ElementOrder::P1);
    let c0 = PeriodicConstraint::build(&s0, &map0, None).unwrap();
    let eps0 = vec![1.0; mesh0.n_tets()];
    assert!(matches!(
        BlochCell::new::<B>(
            &mesh0,
            &PecCavityMaterials::Isotropic(&eps0),
            &c0,
            &device()
        ),
        Err(BlochError::InvalidInput(_))
    ));
}

/// The constant-node pin and the static flag are structural (issue #869):
/// node 0 is pinned (`rank G_r = n_nodes_red − 1`) exactly when `k` is
/// Γ-equivalent (`P(k) = P(0)`) and no node is eliminated. Close to (not
/// at) a Γ-equivalent point the full rank is deflated through the rank-one
/// completion, with no `‖G_r·1‖` tolerance, and no mode is static. Between
/// PEC lids there is never a pin. Inside `near_gamma_min_phase` the solve
/// is a typed refusal naming the threshold.
#[test]
fn bloch_pin_and_static_flag_are_structural() {
    let c = cell(
        renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 869),
        &[0, 1, 2],
        false,
    );
    let nn = c.constraint.nodes().n_reduced();
    let s = BlochSettings::new(4);
    let min = c.cell.near_gamma_min_phase();
    assert!(min > 0.0 && min < 1e-4, "near_gamma_min_phase {min}");
    for (k, gamma) in [
        ([0.0; 3], true),
        ([2.0 * PI, -2.0 * PI, 0.0], true),
        ([1e-16, 0.0, 0.0], true),
        ([1e-3, 0.0, 0.0], false),
        ([2.0 * PI + 1e-3, 0.0, 0.0], false),
        ([0.0, 0.0, 1e-4], false),
        (K_GENERIC, false),
    ] {
        assert_eq!(c.cell.is_gamma_equivalent(k), gamma, "{k:?}");
        let m = c.cell.solve(k, &s).unwrap();
        let n_static = m.modes.iter().filter(|x| x.is_static).count();
        eprintln!(
            "k = {k:?}: Γ-equivalent {gamma}, rank G_r {} / {nn}, static {n_static}, ω₀ {:.3e}",
            m.n_gradient, m.modes[0].omega
        );
        if gamma {
            assert_eq!(m.n_gradient, nn - 1, "{k:?}: pinned");
            assert_eq!(n_static, 3, "{k:?}: three harmonic fields");
        } else {
            assert_eq!(m.n_gradient, nn, "{k:?}: G_r injective, fully deflated");
            assert_eq!(n_static, 0, "{k:?}: no harmonic field off the lattice");
        }
    }
    // Inside the threshold (above round-off): typed, loud, names it.
    let k = [0.1 * min, 0.0, 0.0];
    assert!(!c.cell.is_gamma_equivalent(k));
    match c.cell.solve(k, &s) {
        Err(BlochError::InvalidInput(msg)) => {
            assert!(msg.contains("near_gamma_min_phase"), "{msg}")
        }
        other => panic!("want InvalidInput, got {:?}", other.map(|m| m.modes.len())),
    }

    // PEC lids: nodes are eliminated, so no pin at any k; E = ẑ is static
    // only at Γ.
    let lids = cell(box_tet_mesh([3, 3, 2], [1.0, 1.0, 0.5]), &[0, 1], true);
    let nl = lids.constraint.nodes().n_reduced();
    let g = lids.cell.solve([0.0; 3], &s).unwrap();
    assert_eq!(g.n_gradient, nl);
    assert_eq!(g.modes.iter().filter(|x| x.is_static).count(), 1);
    let near = lids.cell.solve([1e-3, 0.0, 0.0], &s).unwrap();
    assert_eq!(near.n_gradient, nl);
    assert!(near.modes.iter().all(|x| !x.is_static));
}

/// The two Γ tests agree at any `|G|` (issue #915): a `k` whose wrapped
/// Bloch phase distance is 0 is Γ-equivalent, and `solve` treats it as the
/// reciprocal lattice vector it is. At `k = (2π·64, 0, 0)` the round-off
/// of `sin(k·d)` is above the absolute `1e-14` phase-factor snap, which
/// used to make `solve` refuse it as "0 rad from a lattice vector without
/// being one". The translations here are inferred (least squares), on a
/// non-unit cell too. A refusal names the nearest `G`.
#[test]
fn gamma_predicates_agree_at_large_lattice_vectors() {
    let s = BlochSettings::new(4);
    let unit = cell(
        renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 915),
        &[0, 1, 2],
        false,
    );
    let nn = unit.constraint.nodes().n_reduced();
    let g0 = unit.cell.solve([0.0; 3], &s).unwrap();
    for k in [
        [2.0 * PI * 16.0, 0.0, 0.0],
        [2.0 * PI * 64.0, 0.0, 0.0],
        [2.0 * PI * 64.0, -2.0 * PI * 1000.0, 2.0 * PI * 7.0],
    ] {
        // A power-of-two multiple of the f64 2π wraps to exactly 0; any
        // other multiple to round-off (9e-13 rad at 2π·1000), inside the
        // |θ|-scaled lattice tolerance.
        let rho = unit.cell.gamma_phase_distance(k);
        if k[1] == 0.0 {
            assert_eq!(rho, 0.0, "{k:?}: wrapped phase distance");
            assert_eq!(unit.cell.gamma_distance(k), 0.0, "{k:?}");
        }
        assert!(rho < 1e-11, "{k:?}: wrapped phase distance {rho:e}");
        assert!(unit.cell.is_gamma_equivalent(k), "{k:?} is Γ-equivalent");
        assert!(!unit.cell.phased_constraint(k).has_complex_phase(), "{k:?}");
        let m = unit.cell.solve(k, &s).unwrap();
        assert_eq!(m.n_gradient, nn - 1, "{k:?}: pinned");
        assert_eq!(m.modes.iter().filter(|x| x.is_static).count(), 3, "{k:?}");
        assert!(
            (m.modes[3].omega - g0.modes[3].omega).abs() < 1e-9,
            "{k:?}: band 3 {} vs Γ {}",
            m.modes[3].omega,
            g0.modes[3].omega
        );
    }
    // Off the lattice at large |G| the predicates still agree, and the
    // solve is an ordinary near-Γ solve.
    let k = [2.0 * PI * 64.0 + 1e-3, 0.0, 0.0];
    assert!(!unit.cell.is_gamma_equivalent(k));
    assert!((unit.cell.gamma_phase_distance(k) - 1e-3).abs() < 1e-12);
    let m = unit.cell.solve(k, &s).unwrap();
    assert_eq!(m.n_gradient, nn, "{k:?}: fully deflated");
    assert!(m.modes.iter().all(|x| !x.is_static));
    // Never "not Γ-equivalent at distance 0": for every probe, either the
    // point is Γ-equivalent or its distance is positive.
    for e in [0.0, 1e-16, 1e-15, 1e-14, 1e-13, 1e-12, 1e-10] {
        for g in [0.0, 2.0 * PI, 2.0 * PI * 64.0, 2.0 * PI * 4096.0] {
            let k = [g + e, 0.0, 0.0];
            assert!(
                unit.cell.is_gamma_equivalent(k) || unit.cell.gamma_phase_distance(k) > 0.0,
                "{k:?}"
            );
        }
    }

    // A non-unit cell with inferred translations: the nominal k = 2π·m/L
    // per axis is Γ-equivalent.
    let l = [0.7, 1.3, 2.9];
    let odd = cell(
        renumber(&box_tet_mesh([2, 3, 4], l), 916),
        &[0, 1, 2],
        false,
    );
    for m in [1.0, 3.0, 64.0] {
        let k = [2.0 * PI * m / l[0], -2.0 * PI * m / l[1], 2.0 * PI / l[2]];
        assert!(
            odd.cell.is_gamma_equivalent(k),
            "{k:?}: phase distance {:e}",
            odd.cell.gamma_phase_distance(k)
        );
        let r = odd.cell.solve(k, &s).unwrap();
        assert_eq!(r.modes.iter().filter(|x| x.is_static).count(), 3, "{k:?}");
    }

    // A refusal names the distance threshold and the nearest G.
    let g = [2.0 * PI * 3.0 / l[0], 0.0, -2.0 * PI / l[2]];
    let k = [g[0], g[1], g[2] + 0.1 * odd.cell.near_gamma_min_distance()];
    assert!(!odd.cell.is_gamma_equivalent(k));
    match odd.cell.solve(k, &s) {
        Err(BlochError::InvalidInput(msg)) => {
            eprintln!("{msg}");
            assert!(msg.contains("near_gamma_min_distance"), "{msg}");
            assert!(msg.contains("near_gamma_min_phase"), "{msg}");
            // G ≈ [26.927937030769655, 0.0, -2.166615623…].
            assert!(msg.contains("G ≈ [26.9279"), "{msg}");
            assert!(msg.contains("-2.1666"), "{msg}");
        }
        other => panic!("want InvalidInput, got {:?}", other.map(|m| m.modes.len())),
    }
}
