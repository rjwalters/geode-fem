//! Golden 6 of issue #839 (Epic #837 Phase 1): a **periodic manufactured
//! solution** through the periodic direct driven solve, plus the driven
//! composition rules.
//!
//! # Manufactured solution
//!
//! On the box `[0, a] × [0, b] × [0, c]`, periodic in x and y with PEC lids
//! at `z = 0, c`, take
//!
//! ```text
//! E = ẑ ψ,   ψ = sin(2πx/a) cos(2πy/b) + 1/2.
//! ```
//!
//! It is periodic in x and y, has no tangential component on the lids
//! (`E_x = E_y = 0`), and is divergence-free (`∂_z ψ = 0`), so
//! `∇×∇×E = −ΔE = ẑ (2π)²(1/a² + 1/b²) (ψ − 1/2)`. The constant part is the
//! harmonic field `E = ẑ` of this domain (golden 3), which the periodic
//! space must carry; a PEC (non-periodic) side wall would kill it. With the
//! strong form `∇×∇×E − ω²E = iωJ` (natural units, `exp(+jωt)`):
//!
//! ```text
//! J = −i [∇×∇×E − ω² E] / ω.
//! ```
//!
//! The discrete solution is compared with the Whitney interpolant of `E` in
//! the **energy norm** `‖v‖² = vᴴ (K + M) v` (and in the `M` norm), with an
//! O(h) acceptance on the energy norm over three refinements.
//!
//! Also: an empty periodic map reproduces `DrivenOperator::solve_at` bit
//! for bit; a lumped port on a periodic face, and a constraint built with
//! a weaker PEC mask than the operator, are typed errors.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::f64::consts::PI;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use faer::sparse::SparseColMat;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::{PeriodicConstraint, PeriodicError};
use geode_core::driven::periodic::PeriodicDrivenOperator;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator};
use geode_core::eigen::pec_cavity::{PecCavityMaterials, assemble_lossless_pencil_with_materials};
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

const A: f64 = 1.0;
const B_Y: f64 = 1.0;
const C: f64 = 0.5;
const OMEGA: f64 = 2.0;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// 4-point Gauss–Legendre on [0, 1].
const GAUSS_4: [(f64, f64); 4] = [
    (0.5 - 0.430568155797026, 0.173927422568727),
    (0.5 - 0.169990521792428, 0.326072577431273),
    (0.5 + 0.169990521792428, 0.326072577431273),
    (0.5 + 0.430568155797026, 0.173927422568727),
];

fn psi_osc(x: f64, y: f64) -> f64 {
    (2.0 * PI * x / A).sin() * (2.0 * PI * y / B_Y).cos()
}

fn psi(x: f64, y: f64) -> f64 {
    psi_osc(x, y) + 0.5
}

/// Whitney interpolant `∫_e E · dl` (low → high global node).
fn interpolant(mesh: &TetMesh) -> Vec<f64> {
    mesh.edges()
        .iter()
        .map(|e| {
            let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
            let dz = q[2] - p[2];
            if dz == 0.0 {
                return 0.0;
            }
            GAUSS_4
                .iter()
                .map(|&(t, w)| w * psi(p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])))
                .sum::<f64>()
                * dz
        })
        .collect()
}

fn lid_mask(mesh: &TetMesh, space: &HcurlSpace) -> Vec<bool> {
    let lo = boundary_triangles_on_plane(mesh, 2, 0.0, 1e-9);
    let hi = boundary_triangles_on_plane(mesh, 2, C, 1e-9);
    space.pec_interior_mask(mesh, &[&lo, &hi]).unwrap()
}

fn source(mesh: &TetMesh) -> CurrentSource {
    let lap = (2.0 * PI).powi(2) * (1.0 / (A * A) + 1.0 / (B_Y * B_Y));
    CurrentSource::from_centroids(mesh, |c| {
        let curlcurl = lap * psi_osc(c[0], c[1]);
        let rhs = curlcurl - OMEGA * OMEGA * psi(c[0], c[1]);
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new(0.0, -rhs / OMEGA),
        ]
    })
}

fn quad_form(a: &SparseColMat<usize, f64>, v: &[c64]) -> f64 {
    let mut acc = 0.0;
    for j in 0..a.ncols() {
        for (i, &x) in a.as_ref().row_idx_of_col(j).zip(a.as_ref().val_of_col(j)) {
            let p = v[i].conj() * v[j] * x;
            acc += p.re;
        }
    }
    acc
}

/// Relative (energy, M) errors of the periodic solve at refinement `n`.
fn manufactured_errors(mesh: TetMesh) -> (f64, f64) {
    let mut mesh = mesh;
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = lid_mask(&mesh, &space);
    let c = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        &source(&mesh),
        &device(),
    )
    .unwrap();
    let pop = PeriodicDrivenOperator::new(op, &c).unwrap();
    let sol = pop.solve_at(OMEGA).expect("periodic driven solve");
    assert!(sol.residual_rel < 1e-9, "residual {}", sol.residual_rel);
    assert_eq!(sol.n_interior, c.n_reduced());

    let x_a = interpolant(&mesh);
    let err: Vec<c64> = sol
        .e_edges
        .iter()
        .zip(&x_a)
        .map(|(x, a)| *x - c64::new(*a, 0.0))
        .collect();
    let refv: Vec<c64> = x_a.iter().map(|&a| c64::new(a, 0.0)).collect();
    let ones = vec![1.0; mesh.n_tets()];
    let all = vec![true; mesh.edges().len()];
    let (k, m) = assemble_lossless_pencil_with_materials::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&ones),
        &all,
        &device(),
    )
    .unwrap();
    let energy = ((quad_form(&k, &err) + quad_form(&m, &err))
        / (quad_form(&k, &refv) + quad_form(&m, &refv)))
    .sqrt();
    let mnorm = (quad_form(&m, &err) / quad_form(&m, &refv)).sqrt();
    (energy, mnorm)
}

fn mesh_at(n: usize) -> TetMesh {
    box_tet_mesh([n, n, (n / 2).max(1)], [A, B_Y, C])
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "direct solves are slow in debug; runs in release CI"
)]
fn periodic_manufactured_solution_converges() {
    let levels = [4usize, 8, 16];
    let mut errs = Vec::new();
    for &n in &levels {
        let (e, m) = manufactured_errors(mesh_at(n));
        eprintln!("n = {n:>2}: energy-norm err {e:.4e}, M-norm err {m:.4e}");
        errs.push((e, m));
    }
    let rate = |i: usize, f: fn(&(f64, f64)) -> f64| (f(&errs[i]) / f(&errs[i + 1])).log2();
    let e_rates = [rate(0, |p| p.0), rate(1, |p| p.0)];
    let m_rates = [rate(0, |p| p.1), rate(1, |p| p.1)];
    eprintln!("golden 6: energy-norm rates {e_rates:.3?}, M-norm rates {m_rates:.3?}");
    assert!(errs[1].0 < errs[0].0 && errs[2].0 < errs[1].0, "{errs:?}");
    assert!(
        e_rates.iter().all(|&r| r > 0.9),
        "energy-norm rates {e_rates:?}: want O(h)"
    );
    // The renumbered mesh exercises reversed orientations: same errors.
    let (e_ren, _) = manufactured_errors(renumber(&mesh_at(8), 77));
    assert!(
        (e_ren - errs[1].0).abs() < 1e-8 * errs[1].0,
        "renumbered {e_ren} vs {}",
        errs[1].0
    );
}

/// An empty periodic map is the identity: the periodic solve reproduces
/// `DrivenOperator::solve_at` bit for bit.
#[test]
fn empty_periodic_map_reproduces_the_driven_solve_bit_for_bit() {
    let mut mesh = mesh_at(3);
    let map = PeriodicMap::build(&mut mesh, &[], &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = lid_mask(&mesh, &space);
    let c = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let eps: Vec<c64> = (0..mesh.n_tets())
        .map(|t| c64::new(1.0 + (t % 2) as f64, 0.1))
        .collect();
    let assemble = || {
        DrivenOperator::assemble::<B>(
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: &mask,
            },
            &[],
            &[],
            &source(&mesh),
            &device(),
        )
        .unwrap()
    };
    let plain = assemble().solve_at(OMEGA).unwrap();
    let pop = PeriodicDrivenOperator::new(assemble(), &c).unwrap();
    let per = pop.solve_at(OMEGA).unwrap();
    assert_eq!(plain.n_interior, per.n_interior);
    for (a, b) in plain.e_edges.iter().zip(&per.e_edges) {
        assert_eq!(a.re.to_bits(), b.re.to_bits());
        assert_eq!(a.im.to_bits(), b.im.to_bits());
    }
}

/// Composition errors: a lumped port on a periodic face, and a constraint
/// that keeps a DOF the operator eliminated.
#[test]
fn periodic_driven_composition_errors() {
    let mut mesh = mesh_at(2);
    let pairs = box_periodic_pairs(&mesh, &[0, 1]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let mask = lid_mask(&mesh, &space);
    let c = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let zero = CurrentSource::from_centroids(&mesh, |_| [c64::new(0.0, 0.0); 3]);
    let x0 = boundary_triangles_on_plane(&mesh, 0, 0.0, 1e-9);
    let port = LumpedPort {
        faces: &x0[..1],
        e_hat: [0.0, 0.0, 1.0],
        resistance: 50.0,
        width: 0.1,
        length: 0.1,
        v_inc: c64::new(1.0, 0.0),
    };
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port],
        &[],
        &zero,
        &device(),
    )
    .unwrap();
    assert!(matches!(
        PeriodicDrivenOperator::new(op, &c).err(),
        Some(PeriodicError::PortOnPeriodicFace { port: 0 })
    ));

    // Operator with PEC lids, constraint without: rejected.
    let loose = PeriodicConstraint::build(&space, &map, None).unwrap();
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        &zero,
        &device(),
    )
    .unwrap();
    assert!(matches!(
        PeriodicDrivenOperator::new(op, &loose).err(),
        Some(PeriodicError::Mismatch(_))
    ));
}
