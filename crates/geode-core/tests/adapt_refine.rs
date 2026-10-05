//! Goldens of issue #860 (Epic #835 Phase 2): conforming AMP bisection
//! ([`geode_core::adapt::refine`]).
//!
//! 1. **Uniform DOF counts.** Three uniform bisection levels of
//!    `cube_tet_mesh(n)` reproduce the node, tet, edge and face counts of
//!    `cube_tet_mesh(2n)`, so the p=1 and p=2 `HcurlSpace` DOF counts are
//!    known in closed form.
//! 2. **Nesting.** `Pᵀ K_h P = K_H` and `Pᵀ M_h P = M_H` (relative
//!    Frobenius error ≤ 1e-12) for p=1 and p=2. Checked on the cube
//!    (uniform and random local refinement), on a non-Kuhn 5-tet cube, and
//!    on a gmsh fixture (the spiral smoke mesh).
//! 3. **Conformity.** Every interior face is shared by exactly two tets, the
//!    Euler characteristic is unchanged, and the boundary area is conserved.
//! 4. **Tag conservation.** The volume per 3-D group and the area per 2-D
//!    group are conserved to round-off on the spiral fixture.
//! 5. **Quality.** Min dihedral angle and aspect ratio are reported over 8
//!    random-marking cycles. Finitely many similarity classes are checked
//!    numerically: no new classes appear past a fixed depth.
//! 6. **Periodic.** A triply periodic cube, renumbered so that the edge
//!    orientations reverse, is refined at random. #839's matcher accepts
//!    every refined mesh with zero snaps, both through the internal gate and
//!    from scratch with `box_periodic_pairs`. A non-periodic refinement of
//!    the same marks fails the matcher (the tripwire).
//! 7. **Port face.** Refining the middle of a straight waveguide keeps both
//!    port faces exactly planar, and the S-matrix stays within the
//!    discretization band of the analytic `S₂₁ = e^{−jβL}`.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use burn::tensor::backend::BackendTypes;
use faer::c64;
use faer::sparse::SparseColMat;

use geode_core::adapt::refine::{
    BisectionMesh, RefineError, RefineOpts, Refined, mesh_quality, refine,
};
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::nedelec_p2::{P2DofMap, assemble_p2_km};
use geode_core::driven::ports::{
    extruded_rect_waveguide_mesh, project_port_face, solve_wave_port_sweep, wave_port_from_faces,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials};
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::{ascending_vertex_perm, tet_nedelec2_local};
use geode_core::mesh::periodic::{PeriodicMap, PeriodicMatchOptions, box_periodic_pairs};
use geode_core::mesh::{
    TET_LOCAL_EDGES, TaggedTetMesh, TetMesh, cube_tet_mesh, pec_interior_mask_from_triangles,
    read_spiral_smoke_fixture,
};
use geode_core::testing::TestBackend;

use fixtures::{Rng, cube_tet_mesh_5, renumber};

type B = TestBackend;

/// A sparse matrix as `(row, col) → value`.
type SpMap = HashMap<(usize, usize), f64>;
/// Per physical tag: `(element count, total measure)`.
type GroupMap = BTreeMap<i32, (usize, f64)>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn untagged(mesh: TetMesh) -> TaggedTetMesh {
    let n = mesh.n_tets();
    TaggedTetMesh {
        mesh,
        tet_physical_tags: vec![1; n],
        boundary_triangles: Vec::new(),
        triangle_physical_tags: Vec::new(),
    }
}

fn all_tets(m: &TaggedTetMesh) -> Vec<usize> {
    (0..m.mesh.n_tets()).collect()
}

/// A random fraction `frac` of the tets (at least one).
fn random_marks(m: &TaggedTetMesh, frac: f64, rng: &mut Rng) -> Vec<usize> {
    let mut out: Vec<usize> = (0..m.mesh.n_tets())
        .filter(|_| rng.uniform() < frac)
        .collect();
    if out.is_empty() {
        out.push((rng.next_u64() % m.mesh.n_tets() as u64) as usize);
    }
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn tet_volume(mesh: &TetMesh, t: usize) -> f64 {
    let p: Vec<[f64; 3]> = mesh.tets[t]
        .iter()
        .map(|&v| mesh.nodes[v as usize])
        .collect();
    dot(sub(p[1], p[0]), cross(sub(p[2], p[0]), sub(p[3], p[0]))) / 6.0
}

fn tri_area(mesh: &TetMesh, t: &[u32; 3]) -> f64 {
    let p: Vec<[f64; 3]> = t.iter().map(|&v| mesh.nodes[v as usize]).collect();
    let c = cross(sub(p[1], p[0]), sub(p[2], p[0]));
    0.5 * dot(c, c).sqrt()
}

/// p=1 (Whitney) curl-curl and mass matrices as sparse maps, built from the
/// `W` rows of the p=2 element (`W_ab`, `a < b` global, is the Whitney
/// function of the low→high edge). This is independent of the code under
/// test.
fn p1_km(mesh: &TetMesh) -> (SpMap, SpMap) {
    let edges = mesh.edges();
    let lookup: HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0], e[1]), i))
        .collect();
    let mut k = HashMap::new();
    let mut m = HashMap::new();
    for tet in &mesh.tets {
        let perm = ascending_vertex_perm(tet);
        let s: [u32; 4] = std::array::from_fn(|i| tet[perm[i]]);
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[s[i] as usize]);
        let (kl, ml, _) = tet_nedelec2_local(&coords);
        let ge: Vec<usize> = TET_LOCAL_EDGES
            .iter()
            .map(|&(a, b)| lookup[&(s[a], s[b])])
            .collect();
        for i in 0..6 {
            for j in 0..6 {
                *k.entry((ge[i], ge[j])).or_insert(0.0) += kl[2 * i][2 * j];
                *m.entry((ge[i], ge[j])).or_insert(0.0) += ml[2 * i][2 * j];
            }
        }
    }
    (k, m)
}

fn csc_to_map(a: &SparseColMat<usize, f64>) -> HashMap<(usize, usize), f64> {
    let r = a.as_ref();
    let (cp, ri, va) = (r.col_ptr(), r.row_idx(), r.val());
    let mut out = HashMap::new();
    for j in 0..r.ncols() {
        for p in cp[j]..cp[j + 1] {
            *out.entry((ri[p], j)).or_insert(0.0) += va[p];
        }
    }
    out
}

fn p2_km(mesh: &TetMesh) -> (SpMap, SpMap) {
    let dofs = P2DofMap::build(mesh);
    let (k, m) = assemble_p2_km(mesh, &dofs, &vec![1.0; mesh.n_tets()]);
    (csc_to_map(&k), csc_to_map(&m))
}

/// Rows of a sparse prolongation: `rows[i] = [(j, P_ij)]`.
fn rows_of(p: &SparseColMat<usize, f64>) -> Vec<Vec<(usize, f64)>> {
    let r = p.as_ref();
    let (cp, ri, va) = (r.col_ptr(), r.row_idx(), r.val());
    let mut rows = vec![Vec::new(); r.nrows()];
    for j in 0..r.ncols() {
        for q in cp[j]..cp[j + 1] {
            rows[ri[q]].push((j, va[q]));
        }
    }
    rows
}

/// `‖Pᵀ A_h P − A_H‖_F / ‖A_H‖_F`.
fn galerkin_error(
    p: &SparseColMat<usize, f64>,
    a_h: &HashMap<(usize, usize), f64>,
    a_coarse: &HashMap<(usize, usize), f64>,
) -> f64 {
    let rows = rows_of(p);
    let mut g: HashMap<(usize, usize), f64> = HashMap::new();
    for (&(i, j), &v) in a_h {
        for &(ci, pi) in &rows[i] {
            for &(cj, pj) in &rows[j] {
                *g.entry((ci, cj)).or_insert(0.0) += pi * v * pj;
            }
        }
    }
    let keys: HashSet<(usize, usize)> = g.keys().chain(a_coarse.keys()).copied().collect();
    let mut num = 0.0;
    let mut den = 0.0;
    for k in keys {
        let a = g.get(&k).copied().unwrap_or(0.0);
        let b = a_coarse.get(&k).copied().unwrap_or(0.0);
        num += (a - b) * (a - b);
        den += b * b;
    }
    (num / den).sqrt()
}

/// Assert `Pᵀ K_h P = K_H` and `Pᵀ M_h P = M_H` at p=1 (and p=2 if asked).
fn assert_nested(label: &str, coarse: &TetMesh, r: &Refined, p2: bool) {
    let fine = &r.mesh.mesh;
    let (kh, mh) = p1_km(fine);
    let (kc, mc) = p1_km(coarse);
    let p = &r.edge_prolongation;
    assert_eq!(p.nrows(), fine.edges().len());
    assert_eq!(p.ncols(), coarse.edges().len());
    let ek = galerkin_error(p, &kh, &kc);
    let em = galerkin_error(p, &mh, &mc);
    eprintln!("{label}: p=1 nesting  K {ek:.2e}  M {em:.2e}");
    assert!(
        ek <= 1e-12 && em <= 1e-12,
        "{label}: p=1 K {ek:e}, M {em:e}"
    );
    if p2 {
        let p = r
            .hcurl_prolongation(coarse, ElementOrder::P2)
            .expect("p=2 prolongation");
        let (kh, mh) = p2_km(fine);
        let (kc, mc) = p2_km(coarse);
        assert_eq!(
            p.nrows(),
            HcurlSpace::build(fine, ElementOrder::P2).n_dofs()
        );
        assert_eq!(
            p.ncols(),
            HcurlSpace::build(coarse, ElementOrder::P2).n_dofs()
        );
        let ek = galerkin_error(&p, &kh, &kc);
        let em = galerkin_error(&p, &mh, &mc);
        eprintln!("{label}: p=2 nesting  K {ek:.2e}  M {em:.2e}");
        assert!(
            ek <= 1e-12 && em <= 1e-12,
            "{label}: p=2 K {ek:e}, M {em:e}"
        );
    }
}

/// Face multiplicities: interior faces must have exactly two tets.
/// Euler characteristic `V − E + F − T` (1 for a ball, 2 for a shell).
fn euler(mesh: &TetMesh) -> i64 {
    mesh.n_nodes() as i64 - mesh.edges().len() as i64 + mesh.faces().len() as i64
        - mesh.n_tets() as i64
}

/// Face multiplicities (an interior face has exactly two tets) and the
/// Euler characteristic `chi` of the coarse mesh, which a conforming
/// refinement preserves (a hanging node would break it).
fn assert_conforming_chi(label: &str, mesh: &TetMesh, chi: i64) {
    let mut count: HashMap<[u32; 3], u32> = HashMap::new();
    for tet in &mesh.tets {
        for f in [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]] {
            let mut t = [tet[f[0]], tet[f[1]], tet[f[2]]];
            t.sort_unstable();
            *count.entry(t).or_insert(0) += 1;
        }
    }
    assert!(
        count.values().all(|&c| c == 1 || c == 2),
        "{label}: a face is shared by more than two tets"
    );
    assert_eq!(euler(mesh), chi, "{label}: Euler characteristic changed");
}

/// [`assert_conforming_chi`] for a mesh of a ball (`χ = 1`).
fn assert_conforming(label: &str, mesh: &TetMesh) {
    assert_conforming_chi(label, mesh, 1);
}

fn boundary_area(mesh: &TetMesh) -> f64 {
    mesh.boundary_faces()
        .iter()
        .map(|t| tri_area(mesh, t))
        .sum()
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

// ---------------------------------------------------------------------------
// 1. Uniform refinement reproduces known DOF counts
// ---------------------------------------------------------------------------

#[test]
fn uniform_bisection_reproduces_kuhn_counts() {
    for n in [1usize, 2, 3] {
        let mut bm = BisectionMesh::new(untagged(cube_tet_mesh(n, 1.0))).unwrap();
        for level in 1..=6 {
            let marks = all_tets(bm.mesh());
            let before = bm.mesh().mesh.n_tets();
            let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
            // A Kuhn mesh needs no closure: every level exactly doubles.
            assert_eq!(r.stats.n_tets_after, 2 * before, "n={n} level {level}");
            assert_eq!(r.stats.closure_ratio, 1.0, "n={n} level {level}");
            assert_conforming(&format!("kuhn n={n} level {level}"), &r.mesh.mesh);
            if level % 3 == 0 {
                let m = 1usize << (level / 3);
                let want = cube_tet_mesh(m * n, 1.0);
                let got = &r.mesh.mesh;
                assert_eq!(got.n_nodes(), want.n_nodes(), "n={n} level {level}");
                assert_eq!(got.n_tets(), want.n_tets(), "n={n} level {level}");
                assert_eq!(got.edges().len(), want.edges().len(), "n={n} level {level}");
                assert_eq!(got.faces().len(), want.faces().len(), "n={n} level {level}");
                // Closed form: E(N) = 3N(N+1)² + 3N²(N+1) + N³.
                let nn = m * n;
                let e = 3 * nn * (nn + 1).pow(2) + 3 * nn * nn * (nn + 1) + nn.pow(3);
                let f = want.faces().len();
                let s1 = HcurlSpace::build(got, ElementOrder::P1);
                let s2 = HcurlSpace::build(got, ElementOrder::P2);
                assert_eq!(s1.n_dofs(), e);
                assert_eq!(s2.n_dofs(), 2 * e + 2 * f);
                // Same similarity class as the structured mesh.
                let q = r.quality;
                let qw = mesh_quality(&want);
                eprintln!(
                    "kuhn n={n} level {level}: {} tets, p1 {} / p2 {} DOFs, min dihedral {:.4}° \
                     (structured {:.4}°)",
                    got.n_tets(),
                    s1.n_dofs(),
                    s2.n_dofs(),
                    q.min_dihedral_deg,
                    qw.min_dihedral_deg
                );
                assert!((q.min_dihedral_deg - qw.min_dihedral_deg).abs() < 1e-9);
                assert!((q.max_aspect_ratio - qw.max_aspect_ratio).abs() < 1e-9);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Nested prolongation: Pᵀ K_h P = K_H
// ---------------------------------------------------------------------------

#[test]
fn nested_prolongation_cube_uniform_and_local() {
    let coarse = untagged(cube_tet_mesh(2, 1.0));
    let r = refine(&coarse, &all_tets(&coarse), &RefineOpts::default()).unwrap();
    assert_nested("cube uniform", &coarse.mesh, &r, true);

    let mut bm = BisectionMesh::new(coarse).unwrap();
    let mut rng = Rng::new(860);
    for cycle in 0..4 {
        let prev = bm.mesh().mesh.clone();
        let marks = random_marks(bm.mesh(), 0.15, &mut rng);
        let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
        assert_conforming(&format!("cube local {cycle}"), &r.mesh.mesh);
        assert_nested(&format!("cube local {cycle}"), &prev, &r, cycle < 2);
        // P1 nodal prolongation reproduces linear functions exactly.
        let f = |p: [f64; 3]| 1.0 + 2.0 * p[0] - 3.0 * p[1] + 0.5 * p[2];
        let uc: Vec<f64> = prev.nodes.iter().map(|&p| f(p)).collect();
        let rows = rows_of(&r.node_prolongation);
        for (i, row) in rows.iter().enumerate() {
            let v: f64 = row.iter().map(|&(j, w)| w * uc[j]).sum();
            assert!((v - f(r.mesh.mesh.nodes[i])).abs() < 1e-13);
        }
    }
}

#[test]
fn nested_prolongation_non_kuhn_five_tet_cube() {
    // The 5-tet split exercises the A / O / P initial types of AMP.
    let coarse = untagged(cube_tet_mesh_5(2, 1.0));
    let mut bm = BisectionMesh::new(coarse).unwrap();
    let mut rng = Rng::new(5);
    for cycle in 0..5 {
        let prev = bm.mesh().mesh.clone();
        let marks = random_marks(bm.mesh(), 0.2, &mut rng);
        let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
        assert_conforming(&format!("5-tet cycle {cycle}"), &r.mesh.mesh);
        assert!(rel(boundary_area(&r.mesh.mesh), 6.0) < 1e-13);
        assert_nested(&format!("5-tet cycle {cycle}"), &prev, &r, cycle == 0);
    }
}

#[test]
fn nested_prolongation_gmsh_spiral_fixture() {
    let fx = read_spiral_smoke_fixture().expect("spiral smoke fixture");
    let coarse = TaggedTetMesh {
        mesh: fx.mesh,
        tet_physical_tags: fx.tet_physical_tags,
        boundary_triangles: fx.boundary_triangles,
        triangle_physical_tags: fx.triangle_physical_tags,
    };
    let mut bm = BisectionMesh::new(coarse).unwrap();
    let mut rng = Rng::new(3);
    for cycle in 0..2 {
        let prev = bm.mesh().mesh.clone();
        let marks = random_marks(bm.mesh(), 0.05, &mut rng);
        let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
        eprintln!(
            "spiral cycle {cycle}: {} → {} tets, {} marked, closure ratio {:.2}",
            r.stats.n_tets_before, r.stats.n_tets_after, r.stats.n_marked, r.stats.closure_ratio
        );
        assert_nested(&format!("spiral cycle {cycle}"), &prev, &r, cycle == 0);
    }
}

// ---------------------------------------------------------------------------
// 3–4. Conformity and tag conservation on a gmsh fixture
// ---------------------------------------------------------------------------

fn group_measures(m: &TaggedTetMesh) -> (GroupMap, GroupMap) {
    let mut vol: BTreeMap<i32, (usize, f64)> = BTreeMap::new();
    for (t, &tag) in m.tet_physical_tags.iter().enumerate() {
        let e = vol.entry(tag).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += tet_volume(&m.mesh, t).abs();
    }
    let mut area: BTreeMap<i32, (usize, f64)> = BTreeMap::new();
    for (tri, &tag) in m.boundary_triangles.iter().zip(&m.triangle_physical_tags) {
        let e = area.entry(tag).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += tri_area(&m.mesh, tri);
    }
    (vol, area)
}

#[test]
fn tags_volumes_and_areas_are_conserved_on_gmsh_fixture() {
    let fx = read_spiral_smoke_fixture().expect("spiral smoke fixture");
    let coarse = TaggedTetMesh {
        mesh: fx.mesh,
        tet_physical_tags: fx.tet_physical_tags,
        boundary_triangles: fx.boundary_triangles,
        triangle_physical_tags: fx.triangle_physical_tags,
    };
    let (v0, a0) = group_measures(&coarse);
    let ba0 = boundary_area(&coarse.mesh);
    let chi0 = euler(&coarse.mesh);
    let orient0: Vec<f64> = (0..coarse.mesh.n_tets())
        .map(|t| tet_volume(&coarse.mesh, t).signum())
        .collect();
    let mut bm = BisectionMesh::new(coarse.clone()).unwrap();
    let mut rng = Rng::new(11);
    // Mark tets touching tagged surfaces preferentially (ports / walls).
    let mut tagged_nodes: HashSet<u32> = HashSet::new();
    for t in &coarse.boundary_triangles {
        tagged_nodes.extend(t.iter().copied());
    }
    for cycle in 0..3 {
        let m = bm.mesh();
        let marks: Vec<usize> = (0..m.mesh.n_tets())
            .filter(|&t| {
                let near = m.mesh.tets[t].iter().any(|v| tagged_nodes.contains(v));
                rng.uniform() < if near { 0.08 } else { 0.02 }
            })
            .collect();
        let prev = bm.mesh().mesh.clone();
        let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
        let fine = &r.mesh;
        assert_eq!(fine.mesh.physical_groups, coarse.mesh.physical_groups);
        assert_eq!(fine.tet_physical_tags.len(), fine.mesh.n_tets());
        assert_conforming_chi(&format!("spiral tags {cycle}"), &fine.mesh, chi0);
        assert!(rel(boundary_area(&fine.mesh), ba0) < 1e-12);
        let (v1, a1) = group_measures(fine);
        assert_eq!(v1.keys().collect::<Vec<_>>(), v0.keys().collect::<Vec<_>>());
        assert_eq!(a1.keys().collect::<Vec<_>>(), a0.keys().collect::<Vec<_>>());
        for (tag, &(n0, x0)) in &v0 {
            let (n1, x1) = v1[tag];
            assert!(n1 >= n0);
            assert!(
                rel(x1, x0) < 1e-12,
                "volume of 3-D group {tag}: {x1} vs {x0}"
            );
        }
        for (tag, &(n0, x0)) in &a0 {
            let (n1, x1) = a1[tag];
            assert!(n1 >= n0);
            assert!(rel(x1, x0) < 1e-12, "area of 2-D group {tag}: {x1} vs {x0}");
        }
        // Children keep the orientation sign of their parent.
        for (t, &p) in r.parent_of_tet.iter().enumerate() {
            assert_eq!(
                tet_volume(&fine.mesh, t).signum(),
                tet_volume(&prev, p).signum()
            );
        }
        eprintln!(
            "spiral tags cycle {cycle}: {} tets, {} tagged triangles, closure ratio {:.2}",
            fine.mesh.n_tets(),
            fine.boundary_triangles.len(),
            r.stats.closure_ratio
        );
        for (tag, (n, _)) in &a1 {
            eprintln!("  2-D group {tag}: {} → {n} triangles", a0[tag].0);
        }
    }
    // Orientation: every fine tet has the sign of its root (track through
    // the hierarchy by re-refining from scratch once).
    let r = refine(&coarse, &[0, 1, 2], &RefineOpts::default()).unwrap();
    for (t, &p) in r.parent_of_tet.iter().enumerate() {
        assert_eq!(tet_volume(&r.mesh.mesh, t).signum(), orient0[p]);
    }
}

// ---------------------------------------------------------------------------
// 5. Mesh quality over repeated refinement
// ---------------------------------------------------------------------------

#[test]
fn quality_is_bounded_over_eight_random_cycles() {
    // Bars: the Kuhn cube keeps its similarity class (≥ 0.5 × the initial
    // minimum, the epic's bar; in fact equal). The 5-tet split contains a
    // regular central tet, and bisecting a regular tet necessarily produces
    // smaller dihedral angles: its first bisections cost 0.46 × the initial
    // minimum (25.2° vs 54.7°), after which the minimum is constant. The
    // 0.5 × bar is therefore not met there; the honest bar is 0.45 × plus
    // "no further degradation".
    for (label, mesh, floor) in [
        ("kuhn cube", cube_tet_mesh(2, 1.0), 0.5),
        ("5-tet cube", cube_tet_mesh_5(2, 1.0), 0.45),
    ] {
        let mut bm = BisectionMesh::new(untagged(mesh)).unwrap();
        let q0 = bm.initial_quality();
        let mut rng = Rng::new(42);
        let mut worst = q0;
        let mut first = q0;
        for cycle in 0..8 {
            let marks = random_marks(bm.mesh(), 0.1, &mut rng);
            let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
            let q = r.quality;
            if cycle == 0 {
                first = q;
            }
            worst.min_dihedral_deg = worst.min_dihedral_deg.min(q.min_dihedral_deg);
            worst.max_aspect_ratio = worst.max_aspect_ratio.max(q.max_aspect_ratio);
            eprintln!(
                "{label} cycle {cycle}: {} tets, min dihedral {:.3}° (initial {:.3}°), \
                 max aspect {:.3} (initial {:.3}), closure ratio {:.2}",
                q.n_tets,
                q.min_dihedral_deg,
                q0.min_dihedral_deg,
                q.max_aspect_ratio,
                q0.max_aspect_ratio,
                r.stats.closure_ratio
            );
            assert_eq!(r.initial_quality, q0);
        }
        // The similarity bound in action: whatever the first bisections
        // cost, later cycles never degrade the worst shape further.
        assert!(
            (worst.min_dihedral_deg - first.min_dihedral_deg).abs() < 1e-9,
            "{label}: min dihedral kept falling after cycle 0: {} vs {}",
            worst.min_dihedral_deg,
            first.min_dihedral_deg
        );
        assert!(
            worst.min_dihedral_deg >= floor * q0.min_dihedral_deg,
            "{label}: min dihedral {} < {floor} × initial {}",
            worst.min_dihedral_deg,
            q0.min_dihedral_deg
        );
    }
}

/// Similarity signature: the six edge lengths, sorted and normalised by the
/// longest, rounded to 1e-7.
fn signatures(mesh: &TetMesh) -> BTreeSet<[i64; 6]> {
    mesh.tets
        .iter()
        .map(|tet| {
            let mut l: Vec<f64> = TET_LOCAL_EDGES
                .iter()
                .map(|&(a, b)| {
                    let d = sub(mesh.nodes[tet[a] as usize], mesh.nodes[tet[b] as usize]);
                    dot(d, d).sqrt()
                })
                .collect();
            l.sort_by(f64::total_cmp);
            let m = l[5];
            std::array::from_fn(|i| (l[i] / m * 1e7).round() as i64)
        })
        .collect()
}

#[test]
fn similarity_classes_are_finite() {
    // A generic (irregular) single tet: AMP's similarity bound says its
    // descendants fall into finitely many classes.
    let mesh = TetMesh {
        nodes: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.1, 0.05],
            [0.3, 0.9, 0.12],
            [0.25, 0.2, 0.8],
        ],
        tets: vec![[0, 1, 2, 3]],
        physical_groups: BTreeMap::new(),
    };
    let mut bm = BisectionMesh::new(untagged(mesh)).unwrap();
    let mut seen: BTreeSet<[i64; 6]> = BTreeSet::new();
    let mut per_level = Vec::new();
    for level in 1..=15 {
        let marks = all_tets(bm.mesh());
        let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
        let s = signatures(&r.mesh.mesh);
        let new = s.difference(&seen).count();
        seen.extend(s);
        per_level.push(new);
        eprintln!(
            "level {level}: {} tets, {new} new classes ({} total), min dihedral {:.3}°",
            r.mesh.mesh.n_tets(),
            seen.len(),
            r.quality.min_dihedral_deg
        );
    }
    // No new similarity class in the last six levels. (Measured: 36
    // classes, all reached by level 6; pinned as a regression bound.)
    assert!(
        per_level[9..].iter().all(|&n| n == 0),
        "new classes per level: {per_level:?}"
    );
    assert!(seen.len() <= 36, "{} similarity classes", seen.len());
}

// ---------------------------------------------------------------------------
// 6. Periodic: mirrored refinement, gated by #839's matcher
// ---------------------------------------------------------------------------

#[test]
fn refined_periodic_cube_still_matches() {
    // Renumbered so that some periodic edge pairs have reversed
    // orientation (the #839 golden-2 trap).
    let mut mesh = renumber(&cube_tet_mesh(2, 1.0), 860);
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    let opts = PeriodicMatchOptions::default();
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts).expect("coarse periodic match");
    assert!(map.report().n_orientation_reversed() > 0);
    let mut bm = BisectionMesh::new_periodic(untagged(mesh.clone()), &pairs, &map, opts).unwrap();

    let mut rng = Rng::new(837);
    for cycle in 0..5 {
        // Mark tets touching the x = 0 and y = 1 faces (so the closure has
        // to mirror) plus a random interior sprinkle.
        let m = bm.mesh();
        let marks: Vec<usize> = (0..m.mesh.n_tets())
            .filter(|&t| {
                let on = m.mesh.tets[t].iter().any(|&v| {
                    let p = m.mesh.nodes[v as usize];
                    p[0].abs() < 1e-12 || (p[1] - 1.0).abs() < 1e-12
                });
                rng.uniform() < if on { 0.3 } else { 0.03 }
            })
            .collect();
        let r = bm.refine(&marks, &RefineOpts::default()).unwrap();
        let gate = r.periodic_map.as_ref().expect("gated periodic map");
        assert_eq!(gate.report().n_snapped(), 0);
        assert!(gate.report().warnings().is_empty());
        assert_conforming(&format!("periodic {cycle}"), &r.mesh.mesh);

        // Independent re-match from scratch: the refined faces match by
        // geometry alone.
        let mut fresh = r.mesh.mesh.clone();
        let fresh_pairs = box_periodic_pairs(&fresh, &[0, 1, 2]);
        let fresh_map = PeriodicMap::build(&mut fresh, &fresh_pairs, &opts)
            .expect("refined periodic cube must match");
        assert_eq!(fresh_map.report().n_snapped(), 0);
        for (p, q) in fresh_map.report().pairs.iter().zip(&gate.report().pairs) {
            assert_eq!(p.n_faces, q.n_faces);
            assert_eq!(p.n_nodes, q.n_nodes);
        }
        let n_faces: Vec<usize> = gate.report().pairs.iter().map(|p| p.n_faces).collect();
        eprintln!(
            "periodic cycle {cycle}: {} tets, paired faces per axis {n_faces:?}, closure ratio {:.2}",
            r.mesh.mesh.n_tets(),
            r.stats.closure_ratio
        );
        if cycle == 0 {
            assert_nested("periodic cycle 0", &mesh, &r, false);
        }
    }
    assert!(bm.periodic_map().is_some());
    assert_eq!(bm.periodic_pairs().len(), 3);
}

#[test]
fn non_mirrored_refinement_breaks_the_match_tripwire() {
    // The same kind of marks without the periodic state: the x = 0 face is
    // refined but x = 1 is not, and #839's matcher rejects the result. This
    // proves the gate (and the mirroring) is load-bearing.
    // Three cycles: the first Kuhn level only splits body diagonals, the
    // second and third reach the x = 0 face edges.
    let mut bm = BisectionMesh::new(untagged(cube_tet_mesh(2, 1.0))).unwrap();
    for _ in 0..3 {
        let mesh = &bm.mesh().mesh;
        let marks: Vec<usize> = (0..mesh.n_tets())
            .filter(|&t| {
                mesh.tets[t]
                    .iter()
                    .any(|&v| mesh.nodes[v as usize][0] == 0.0)
            })
            .collect();
        bm.refine(&marks, &RefineOpts::default()).unwrap();
    }
    let mut fine = bm.mesh().mesh.clone();
    let pairs = box_periodic_pairs(&fine, &[0]);
    assert!(PeriodicMap::build(&mut fine, &pairs, &PeriodicMatchOptions::default()).is_err());
}

#[test]
fn periodic_map_of_another_mesh_is_rejected() {
    let mut a = cube_tet_mesh(2, 1.0);
    let pairs = box_periodic_pairs(&a, &[0]);
    let map = PeriodicMap::build(&mut a, &pairs, &PeriodicMatchOptions::default()).unwrap();
    let other = untagged(cube_tet_mesh(3, 1.0));
    assert!(matches!(
        BisectionMesh::new_periodic(other, &pairs, &map, PeriodicMatchOptions::default()),
        Err(RefineError::InvalidInput { .. })
    ));
}

// ---------------------------------------------------------------------------
// 7. Port faces stay planar; S unchanged under far-region refinement
// ---------------------------------------------------------------------------

const A: f64 = 2.0;
const B_DIM: f64 = 1.0;
const LEN: f64 = 1.2;
const TAG_PORT_IN: i32 = 11;
const TAG_PORT_OUT: i32 = 12;
const TAG_WALLS: i32 = 13;

fn s_matrix(m: &TaggedTetMesh, omega: f64) -> ([c64; 4], f64) {
    let edges = m.mesh.edges();
    let a_inc = [c64::new(1.0, 0.0)];
    let p_in = wave_port_from_faces(&m.mesh, &edges, &m.triangles_with_tag(TAG_PORT_IN), &a_inc)
        .expect("port_in");
    let p_out = wave_port_from_faces(&m.mesh, &edges, &m.triangles_with_tag(TAG_PORT_OUT), &a_inc)
        .expect("port_out");
    let walls = m.triangles_with_tag(TAG_WALLS);
    let mask = pec_interior_mask_from_triangles(&edges, &[walls.as_slice()]);
    let eps = vec![c64::new(1.0, 0.0); m.mesh.n_tets()];
    let device = <B as BackendTypes>::Device::default();
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let pt = solve_wave_port_sweep::<B>(
        &m.mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[p_in, p_out],
        &[omega],
        &device,
    )
    .expect("wave-port sweep")
    .remove(0);
    ([pt.s[0], pt.s[1], pt.s[2], pt.s[3]], pt.beta[0].re)
}

fn waveguide() -> TaggedTetMesh {
    let g = extruded_rect_waveguide_mesh(8, 4, 4, A, B_DIM, LEN);
    let mut tris = Vec::new();
    let mut tags = Vec::new();
    for (faces, tag) in [
        (&g.port1_faces, TAG_PORT_IN),
        (&g.port2_faces, TAG_PORT_OUT),
        (&g.sidewall_faces, TAG_WALLS),
    ] {
        for t in faces {
            tris.push(*t);
            tags.push(tag);
        }
    }
    let n = g.mesh.n_tets();
    TaggedTetMesh {
        mesh: g.mesh,
        tet_physical_tags: vec![1; n],
        boundary_triangles: tris,
        triangle_physical_tags: tags,
    }
}

/// Refine the tets whose centroid `z` satisfies `select` for `cycles`
/// cycles; check conformity and that both port faces stay exactly planar
/// (bit-exact `z`) with their area conserved. Returns the refined mesh.
fn refine_guide(
    coarse: &TaggedTetMesh,
    cycles: usize,
    select: impl Fn(f64) -> bool,
) -> TaggedTetMesh {
    let mut bm = BisectionMesh::new(coarse.clone()).unwrap();
    for _ in 0..cycles {
        let m = bm.mesh();
        let marks: Vec<usize> = (0..m.mesh.n_tets())
            .filter(|&t| {
                let zc: f64 = m.mesh.tets[t]
                    .iter()
                    .map(|&v| m.mesh.nodes[v as usize][2])
                    .sum::<f64>()
                    / 4.0;
                select(zc)
            })
            .collect();
        bm.refine(&marks, &RefineOpts::default()).unwrap();
    }
    let fine = bm.mesh().clone();
    assert_conforming("waveguide", &fine.mesh);
    for (tag, z) in [(TAG_PORT_IN, 0.0), (TAG_PORT_OUT, LEN)] {
        let tris = fine.triangles_with_tag(tag);
        for t in &tris {
            for &v in t {
                assert_eq!(
                    fine.mesh.nodes[v as usize][2], z,
                    "port {tag} node off its plane"
                );
            }
        }
        let area: f64 = tris.iter().map(|t| tri_area(&fine.mesh, t)).sum();
        assert!(rel(area, A * B_DIM) < 1e-13);
        // The face still projects as a planar port face.
        let proj = project_port_face(&fine.mesh, &tris).expect("planar port face");
        assert!((proj.normal[2].abs() - 1.0).abs() < 1e-12);
        eprintln!(
            "port {tag}: {} → {} triangles",
            coarse.triangles_with_tag(tag).len(),
            tris.len()
        );
    }
    fine
}

/// Straight-section acceptance of `tests/wave_port.rs` for `s`.
fn assert_straight_section(label: &str, s: &[c64; 4], beta: f64) {
    let want = c64::new((-beta * LEN).cos(), (-beta * LEN).sin());
    eprintln!(
        "{label}: |S11| = {:.3e}, S21 = {}, e^(-jβL) = {want}, |S21 - e^(-jβL)| = {:.3e}",
        s[0].norm(),
        s[2],
        (s[2] - want).norm()
    );
    assert!(s[0].norm() < 0.5, "{label}: |S11| = {}", s[0].norm());
    assert!(
        (s[2] - want).norm() < 0.1,
        "{label}: S21 = {} vs {want}",
        s[2]
    );
    assert!(
        (s[1] - s[2]).norm() / s[2].norm() < 0.1,
        "{label}: reciprocity"
    );
}

#[test]
fn port_faces_stay_planar_and_s_is_unchanged_by_far_refinement() {
    let coarse = waveguide();
    let omega = 2.5;
    let (s0, beta0) = s_matrix(&coarse, omega);
    assert_straight_section("coarse", &s0, beta0);

    // The middle third of the guide, far from both ports: the port faces
    // are untouched, so the port modes (β) are bit-identical and S moves
    // only by the volume discretization error.
    let fine = refine_guide(&coarse, 3, |z| z > LEN / 3.0 && z < 2.0 * LEN / 3.0);
    assert_eq!(
        fine.triangles_with_tag(TAG_PORT_IN),
        coarse.triangles_with_tag(TAG_PORT_IN)
    );
    let (s1, beta1) = s_matrix(&fine, omega);
    assert_eq!(beta1, beta0);
    assert_straight_section("far-refined", &s1, beta1);
    let ds: f64 = (0..4).map(|k| (s1[k] - s0[k]).norm()).fold(0.0, f64::max);
    eprintln!(
        "far refinement: {} → {} tets, max |ΔS| = {ds:.3e}",
        coarse.mesh.n_tets(),
        fine.mesh.n_tets()
    );
    assert!(ds < 0.05, "max |ΔS| = {ds}");
}

#[test]
fn refined_port_face_stays_planar_and_s_stays_in_band() {
    let coarse = waveguide();
    let omega = 2.5;
    let (s0, beta0) = s_matrix(&coarse, omega);

    // Refine next to port 1, so its face mesh is itself bisected.
    let fine = refine_guide(&coarse, 3, |z| z < LEN / 4.0);
    assert!(
        fine.triangles_with_tag(TAG_PORT_IN).len() > coarse.triangles_with_tag(TAG_PORT_IN).len(),
        "port 1 face was not refined"
    );
    let (s1, beta1) = s_matrix(&fine, omega);
    assert_straight_section("port-refined", &s1, beta1);
    let ds: f64 = (0..4)
        .map(|k| (s1[k].norm() - s0[k].norm()).abs())
        .fold(0.0, f64::max);
    eprintln!(
        "port refinement: {} → {} tets, β {beta0} → {beta1}, max ||S| change| = {ds:.3e}",
        coarse.mesh.n_tets(),
        fine.mesh.n_tets()
    );
    assert!(ds < 0.05, "max ||S| change| = {ds}");
}
