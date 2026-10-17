//! Golden 3 of issue #839 (Epic #837 Phase 1): the **exact discrete
//! topology** of the periodic-reduced curl-curl operator, plus the inverse
//! tripwires 7(ii) (forced `σ = +1`) and 7(iii) (no corner chaining).
//!
//! # Expected kernel dimensions (relative de Rham cohomology)
//!
//! For lowest-order Whitney forms the discrete complex reproduces the
//! cohomology of the domain exactly, so on the reduced (periodic) complex
//! `dim ker K_r = rank(G_r) + b₁`, with `b₁` the first Betti number of the
//! (relative) domain:
//!
//! - **3-torus** `T³` (x, y, z periodic, no PEC): `rank(G_r) = n_nodes_red −
//!   1` (constants are in the kernel of the gradient) and `b₁(T³) = 3` (the
//!   uniform fields `x̂, ŷ, ẑ`), so `dim ker K_r = n_nodes_red − 1 + 3`.
//! - **x, y periodic, PEC lids at z = 0, c** (`M = T² × [0, c]` relative to
//!   `∂M = T² × {0, c}`): the gradient acts on the interior (non-PEC) nodes
//!   and is injective there, and `H¹(M, ∂M) ≅ H₂(M) ≅ H₂(T²) = ℝ` by
//!   Lefschetz duality — one harmonic field, `E = ẑ`. So `dim ker K_r =
//!   n_interior_nodes_red + 1`.
//!
//! Both are exact integers, counted by a dense SVD of `K_r` (PSD, so its
//! singular values are its eigenvalues) on small meshes — lexicographic and
//! randomly renumbered (golden 2: the renumbered mesh exercises `σ = −1`).
//! The reduced gradient is checked against `G P_node = P_edge G_r` and
//! `K_r G_r = 0`.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use burn::tensor::backend::BackendTypes;
use faer::Mat;
use faer::sparse::SparseColMat;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::PeriodicConstraint;
use geode_core::eigen::pec_cavity::PecCavityMaterials;
use geode_core::eigen::periodic_cavity::assemble_periodic_lossless_pencil;
use geode_core::elements::ElementOrder;
use geode_core::mesh::TetMesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchOptions, boundary_triangles_on_plane, box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{box_tet_mesh, renumber};

type B = TestBackend;

struct Topology {
    kernel: usize,
    n_reduced: usize,
    n_nodes_reduced: usize,
    rank_gr: usize,
    gradient_commutator: f64,
    kgr: f64,
}

fn dense(a: &SparseColMat<usize, f64>) -> Mat<f64> {
    a.to_dense()
}

fn rank_and_kernel(a: &Mat<f64>) -> (usize, usize) {
    let s = a.singular_values().expect("svd");
    let smax = s.iter().copied().fold(0.0, f64::max);
    let rank = s.iter().filter(|&&x| x > 1e-9 * smax).count();
    // Print the gap for the record.
    let above = s
        .iter()
        .copied()
        .filter(|&x| x > 1e-9 * smax)
        .fold(f64::INFINITY, f64::min);
    let below = s
        .iter()
        .copied()
        .filter(|&x| x <= 1e-9 * smax)
        .fold(0.0, f64::max);
    eprintln!(
        "    svd gap: smallest kept {:.3e}, largest dropped {:.3e} (σ_max {:.3e})",
        above, below, smax
    );
    (rank, a.ncols() - rank)
}

fn topology(
    mesh: &mut TetMesh,
    axes: &[usize],
    pec_lids: bool,
    opts: PeriodicMatchOptions,
) -> Topology {
    let pairs = box_periodic_pairs(mesh, axes);
    let map = PeriodicMap::build(mesh, &pairs, &opts).expect("periodic map");
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let mask = if pec_lids {
        let zmax = mesh.nodes.iter().map(|p| p[2]).fold(0.0, f64::max);
        let lo = boundary_triangles_on_plane(mesh, 2, 0.0, 1e-9);
        let hi = boundary_triangles_on_plane(mesh, 2, zmax, 1e-9);
        Some(space.pec_interior_mask(mesh, &[&lo, &hi]).unwrap())
    } else {
        None
    };
    let c = PeriodicConstraint::build(&space, &map, mask.as_deref()).unwrap();
    let eps = vec![1.0; mesh.n_tets()];
    let device = <B as BackendTypes>::Device::default();
    let (k, _m) = assemble_periodic_lossless_pencil::<B>(
        mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &c,
        &device,
    )
    .unwrap();
    let kd = dense(&k);
    let (_, kernel) = rank_and_kernel(&kd);
    let gr = dense(&c.reduced_gradient());
    let (rank_gr, _) = rank_and_kernel(&gr);
    let kgr = (&kd * &gr).norm_max() / kd.norm_max();
    Topology {
        kernel,
        n_reduced: c.n_reduced(),
        n_nodes_reduced: c.nodes().n_reduced(),
        rank_gr,
        gradient_commutator: c.check_gradient_commutes(),
        kgr,
    }
}

fn check_three_torus(mesh: TetMesh, label: &str) {
    let mut mesh = mesh;
    let t = topology(
        &mut mesh,
        &[0, 1, 2],
        false,
        PeriodicMatchOptions::default(),
    );
    eprintln!(
        "{label}: 3-torus n_red = {}, nodes_red = {}, dim ker K_r = {} (want {}), rank G_r = {}",
        t.n_reduced,
        t.n_nodes_reduced,
        t.kernel,
        t.n_nodes_reduced - 1 + 3,
        t.rank_gr
    );
    assert_eq!(
        t.gradient_commutator, 0.0,
        "G P_node = P_edge G_r must hold exactly"
    );
    assert!(t.kgr < 1e-12, "K_r G_r = {:e}", t.kgr);
    assert_eq!(t.rank_gr, t.n_nodes_reduced - 1);
    assert_eq!(t.kernel, t.n_nodes_reduced - 1 + 3, "b₁(T³) = 3");
}

fn check_slab(mesh: TetMesh, label: &str) {
    let mut mesh = mesh;
    let t = topology(&mut mesh, &[0, 1], true, PeriodicMatchOptions::default());
    eprintln!(
        "{label}: xy-periodic + PEC lids n_red = {}, interior nodes_red = {}, dim ker K_r = {} \
         (want {})",
        t.n_reduced,
        t.n_nodes_reduced,
        t.kernel,
        t.n_nodes_reduced + 1
    );
    assert_eq!(t.gradient_commutator, 0.0);
    assert!(t.kgr < 1e-12, "K_r G_r = {:e}", t.kgr);
    assert_eq!(
        t.rank_gr, t.n_nodes_reduced,
        "G_r injective on interior nodes"
    );
    assert_eq!(t.kernel, t.n_nodes_reduced + 1, "H¹(T²×I, ∂) = ℝ (E = ẑ)");
}

#[test]
fn three_torus_kernel_is_gradients_plus_three_harmonic_fields() {
    check_three_torus(box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), "lexicographic 3³");
    check_three_torus(
        box_tet_mesh([3, 4, 3], [1.0, 1.3, 0.8]),
        "lexicographic 3×4×3 box",
    );
    check_three_torus(
        renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 99),
        "renumbered 3³",
    );
}

#[test]
fn xy_periodic_pec_lids_kernel_is_interior_gradients_plus_one() {
    check_slab(box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), "lexicographic 3³");
    check_slab(
        renumber(&box_tet_mesh([3, 4, 2], [1.0, 1.2, 0.6]), 5),
        "renumbered 3×4×2",
    );
}

/// Tripwire 7(ii): on the renumbered mesh, forcing every edge sign to `+1`
/// must break the exact topology (golden 3 fails), proving the orientation
/// path is live.
#[test]
fn forced_unit_orientation_breaks_the_topology() {
    let mut mesh = renumber(&box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]), 99);
    let opts = PeriodicMatchOptions {
        tripwire_force_unit_orientation: true,
        ..Default::default()
    };
    let t = topology(&mut mesh, &[0, 1, 2], false, opts);
    eprintln!(
        "tripwire σ := +1: dim ker K_r = {} vs correct {}; commutator {}",
        t.kernel,
        t.n_nodes_reduced - 1 + 3,
        t.gradient_commutator
    );
    assert!(
        t.gradient_commutator > 0.5,
        "G P_node = P_edge G_r must fail"
    );
    assert_ne!(
        t.kernel,
        t.n_nodes_reduced - 1 + 3,
        "golden 3 must fail with σ := +1"
    );
}

/// Tripwire 7(iii): dropping corner chaining must break golden 3 on the
/// 3-torus.
#[test]
fn dropped_corner_chaining_breaks_the_topology() {
    let mut mesh = box_tet_mesh([3, 3, 3], [1.0, 1.0, 1.0]);
    let opts = PeriodicMatchOptions {
        tripwire_disable_chaining: true,
        ..Default::default()
    };
    let t = topology(&mut mesh, &[0, 1, 2], false, opts);
    eprintln!(
        "tripwire no chaining: dim ker K_r = {} vs correct {} (with these nodes_red)",
        t.kernel,
        t.n_nodes_reduced - 1 + 3
    );
    // The correct reduced complex has 27 nodes and 81 edges; without
    // chaining the corner / box-edge copies stay split, so the reduced
    // complex is not the torus and golden 3's identity fails.
    assert_ne!(t.n_nodes_reduced, 27);
    assert_ne!(t.n_reduced, 81);
    assert_ne!(
        t.kernel,
        t.n_nodes_reduced - 1 + 3,
        "golden 3 must fail without chaining"
    );
}
