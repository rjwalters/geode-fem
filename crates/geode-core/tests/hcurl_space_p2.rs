//! `HcurlSpace` gates (issue #838, Epic #836 Phase 1a): the p=1 tables and
//! mask are the pre-#838 ones bit for bit; the p=2 PEC mask is face-exact
//! (no chord DOF is ever eliminated); `face_dof_transform` reproduces the
//! shared-face trace under all six relabellings; the assembled p=2
//! curl-curl annihilates global P2 gradients on a random tagged mesh; the
//! hierarchical injection `prolong_p1` reproduces the p=1 field.

use std::collections::BTreeSet;

use faer::c64;
use geode_core::assembly::hcurl_space::{HcurlSpace, TetOrientation};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
};
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::tet_barycentric_gradients;
use geode_core::mesh::{TetMesh, cube_tet_mesh, pec_interior_mask_from_triangles};
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Deterministic uniform `[-1, 1)` stream (LCG; no external RNG crate).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// `cube_tet_mesh(n)` with every **interior** node jittered by up to
/// `0.12·h` per axis: an unstructured, flat-sided box (boundary nodes stay
/// put, so the walls stay planar).
fn jittered_cube(n: usize, seed: u64) -> TetMesh {
    let mut mesh = cube_tet_mesh(n, 1.0);
    let h = 1.0 / n as f64;
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        if p.iter().all(|&x| x > 1e-12 && x < 1.0 - 1e-12) {
            for x in p.iter_mut() {
                *x += 0.12 * h * rng.next();
            }
        }
    }
    for (t, tet) in mesh.tets.iter().enumerate() {
        let c: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (_, vol) = tet_barycentric_gradients(&c);
        assert!(vol > 0.0, "jitter inverted tet {t}");
    }
    mesh
}

/// Boundary faces lying in the plane `coord[axis] == value`.
fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.boundary_faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

fn sorted_edge(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

#[test]
fn p1_space_reproduces_the_pre_838_tables_and_mask_bit_for_bit() {
    for (n, mesh) in [
        (2, cube_tet_mesh(2, 1.0)),
        (3, jittered_cube(3, 7)),
        (4, jittered_cube(4, 11)),
    ] {
        let space = HcurlSpace::build(&mesh, ElementOrder::P1);
        let edges = mesh.edges();
        assert_eq!(space.n_dofs(), edges.len(), "n={n}: n_dofs != n_edges");
        assert_eq!(space.edges(), edges.as_slice());
        for (t, row) in mesh.tet_edges().iter().enumerate() {
            let ids: Vec<u32> = row.iter().map(|r| r.0).collect();
            assert_eq!(space.tet_dofs(t), ids.as_slice());
            let signs: [i8; 6] = std::array::from_fn(|i| row[i].1);
            assert_eq!(space.tet_orientation(t), TetOrientation::EdgeSigns(signs));
        }
        // Several tagged wall lists, including partial walls.
        let all = mesh.boundary_faces();
        let z0 = plane_faces(&mesh, 2, 0.0);
        let x1 = plane_faces(&mesh, 0, 1.0);
        for walls in [
            vec![all.as_slice()],
            vec![z0.as_slice()],
            vec![z0.as_slice(), x1.as_slice()],
            vec![],
        ] {
            assert_eq!(
                space.pec_interior_mask(&mesh, &walls),
                pec_interior_mask_from_triangles(&edges, &walls),
                "n={n}: p=1 mask differs from pec_interior_mask_from_triangles"
            );
        }
    }
}

#[test]
fn p2_pec_mask_is_face_exact_and_never_eliminates_chords() {
    for n in [1usize, 2, 4] {
        let mesh = cube_tet_mesh(n, 1.0);
        let space = HcurlSpace::build(&mesh, ElementOrder::P2);
        let walls = mesh.boundary_faces();
        let mask = space.pec_interior_mask(&mesh, &[walls.as_slice()]);

        let wall_faces: BTreeSet<[u32; 3]> = walls.iter().copied().collect();
        let wall_edges: BTreeSet<(u32, u32)> = walls
            .iter()
            .flat_map(|t| {
                [
                    sorted_edge(t[0], t[1]),
                    sorted_edge(t[0], t[2]),
                    sorted_edge(t[1], t[2]),
                ]
            })
            .collect();
        let on_boundary: BTreeSet<u32> = walls.iter().flatten().copied().collect();

        let (mut int_edges, mut int_faces, mut chords) = (0usize, 0usize, 0usize);
        for (ge, e) in space.edges().iter().enumerate() {
            let is_wall = wall_edges.contains(&(e[0], e[1]));
            if !is_wall {
                int_edges += 1;
                if on_boundary.contains(&e[0]) && on_boundary.contains(&e[1]) {
                    chords += 1;
                }
            }
            for &d in space.edge_dofs(ge) {
                assert_eq!(mask[d as usize], !is_wall, "n={n}: edge {e:?}");
            }
        }
        for (gf, f) in space.faces().iter().enumerate() {
            let is_wall = wall_faces.contains(f);
            if !is_wall {
                int_faces += 1;
                if f.iter().all(|v| on_boundary.contains(v)) {
                    chords += 1;
                }
            }
            for &d in space.face_dofs(gf) {
                assert_eq!(mask[d as usize], !is_wall, "n={n}: face {f:?}");
            }
        }
        let kept = mask.iter().filter(|&&k| k).count();
        assert_eq!(
            kept,
            2 * int_edges + 2 * int_faces,
            "n={n}: interior DOF count"
        );
        println!(
            "n={n}: {kept} interior p=2 DOFs = 2·{int_edges} edges + 2·{int_faces} faces; \
             {chords} boundary-vertex chords kept free"
        );

        if n == 1 {
            // The body diagonal (0,0,0)–(1,1,1) joins two boundary nodes but
            // is interior: its (W, Q) must stay free.
            let node = |p: [f64; 3]| {
                mesh.nodes
                    .iter()
                    .position(|q| q.iter().zip(p.iter()).all(|(a, b)| (a - b).abs() < 1e-12))
                    .unwrap() as u32
            };
            let diag = sorted_edge(node([0.0; 3]), node([1.0; 3]));
            let ge = space
                .edges()
                .iter()
                .position(|e| (e[0], e[1]) == diag)
                .expect("body diagonal is a mesh edge");
            assert!(space.edge_dofs(ge).iter().all(|&d| mask[d as usize]));
            // Every interior face of the single hex keeps (φ0, φ1).
            assert!(int_faces > 0);
            assert!(
                chords > 0,
                "n=1 must have chords (the body diagonal at least)"
            );
        }
    }
}

#[test]
fn p2_partial_tagged_walls_leave_other_boundaries_free() {
    let mesh = jittered_cube(3, 5);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let z0 = plane_faces(&mesh, 2, 0.0);
    let mask = space.pec_interior_mask(&mesh, &[z0.as_slice()]);
    let z0_set: BTreeSet<[u32; 3]> = z0.iter().copied().collect();
    for (gf, f) in space.faces().iter().enumerate() {
        let want = !z0_set.contains(f);
        for &d in space.face_dofs(gf) {
            assert_eq!(mask[d as usize], want);
        }
    }
    // A boundary face on another wall is free.
    let x1 = plane_faces(&mesh, 0, 1.0);
    let gf = space.faces().iter().position(|f| *f == x1[0]).unwrap();
    assert!(space.face_dofs(gf).iter().all(|&d| mask[d as usize]));
}

// ---------------------------------------------------------------------------
// face_dof_transform: two-tet shared-face trace, all six relabellings
// ---------------------------------------------------------------------------

/// Barycentrics and their gradients of the tet `v` at `x`.
fn bary_at(v: &[[f64; 3]; 4], x: [f64; 3]) -> ([f64; 4], [[f64; 3]; 4]) {
    let (g, _) = tet_barycentric_gradients(v);
    let d = [x[0] - v[0][0], x[1] - v[0][1], x[2] - v[0][2]];
    let lam: [f64; 4] = std::array::from_fn(|i| {
        let base = if i == 0 { 1.0 } else { 0.0 };
        base + g[i][0] * d[0] + g[i][1] * d[1] + g[i][2] * d[2]
    });
    (lam, g)
}

/// `ψ(x, y, z) = λ_z (λ_x ∇λ_y − λ_y ∇λ_x)` in local vertex indices.
fn psi(lam: &[f64; 4], g: &[[f64; 3]; 4], x: usize, y: usize, z: usize) -> [f64; 3] {
    std::array::from_fn(|d| lam[z] * (lam[x] * g[y][d] - lam[y] * g[x][d]))
}

fn tangential(v: [f64; 3], n: [f64; 3]) -> [f64; 3] {
    let vn = v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
    std::array::from_fn(|d| v[d] - vn * n[d])
}

#[test]
fn face_dof_transform_reproduces_the_shared_face_trace_for_all_six_relabellings() {
    // Shared face F = {p0, p1, p2} (ascending global vertices 0 < 1 < 2);
    // tet A = (p0, p1, p2, p3) on one side, tet B = (p0, p1, p2, p4) on the
    // other. Both tets list the face vertices first, in ascending order.
    let p = [
        [0.0, 0.0, 0.0],
        [1.0, 0.1, 0.0],
        [0.2, 0.9, 0.0],
        [0.3, 0.3, 0.8],
        [0.4, 0.2, -0.7],
    ];
    let ta = [p[0], p[1], p[2], p[3]];
    let tb = [p[0], p[1], p[2], p[4]];
    let normal = [0.0, 0.0, 1.0];
    let samples = [
        [0.3, 0.2, 0.0],
        [0.5, 0.4, 0.0],
        [0.15, 0.6, 0.0],
        [0.4, 0.1, 0.0],
    ];
    let perms: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let mut rng = Lcg(3);
    let mut n_mixing = 0usize;
    for perm in perms {
        let t = HcurlSpace::face_dof_transform(perm);
        let off_diag = t[0][1] != 0.0 || t[1][0] != 0.0;
        if off_diag {
            n_mixing += 1;
        }
        // Random relabelled DOFs c' and the ascending DOFs c = Tᵀ c'.
        let cp = [rng.next(), rng.next()];
        let c = [
            t[0][0] * cp[0] + t[1][0] * cp[1],
            t[0][1] * cp[0] + t[1][1] * cp[1],
        ];
        for x in samples {
            let (la, ga) = bary_at(&ta, x);
            let (lb, gb) = bary_at(&tb, x);
            // Ascending pair in tet A; relabelled pair in tet B.
            let phi = [psi(&la, &ga, 0, 1, 2), psi(&la, &ga, 1, 2, 0)];
            let [a, b, cc] = perm;
            let phip = [psi(&lb, &gb, a, b, cc), psi(&lb, &gb, b, cc, a)];
            // Function-level identity φ'_i = Σ_j T_ij φ_j (traces).
            for i in 0..2 {
                let lhs = tangential(phip[i], normal);
                let rhs = tangential(
                    std::array::from_fn(|d| t[i][0] * phi[0][d] + t[i][1] * phi[1][d]),
                    normal,
                );
                for d in 0..3 {
                    assert!(
                        (lhs[d] - rhs[d]).abs() < 1e-14,
                        "perm {perm:?}: φ'_{i} trace mismatch {lhs:?} vs {rhs:?}"
                    );
                }
            }
            // DOF-level identity Σ c'_i φ'_i = Σ (Tᵀc')_j φ_j on the face.
            let ub = tangential(
                std::array::from_fn(|d| cp[0] * phip[0][d] + cp[1] * phip[1][d]),
                normal,
            );
            let ua = tangential(
                std::array::from_fn(|d| c[0] * phi[0][d] + c[1] * phi[1][d]),
                normal,
            );
            for d in 0..3 {
                assert!((ua[d] - ub[d]).abs() < 1e-14, "perm {perm:?}: DOF map");
            }
        }
    }
    // A raw relabelling genuinely mixes φ0/φ1 for the non-identity
    // relabellings that move the multiplier vertex: the 2×2 is required.
    assert!(
        n_mixing >= 2,
        "expected non-diagonal transforms, got {n_mixing}"
    );
}

#[test]
fn assembled_p2_space_is_tangentially_conforming_across_interior_faces() {
    let mesh = jittered_cube(2, 9);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mut rng = Lcg(21);
    let x: Vec<c64> = (0..space.n_dofs())
        .map(|_| c64::new(rng.next(), rng.next()))
        .collect();
    // Map each face to its incident (tet, local vertex opposite) pairs.
    let mut owners: std::collections::BTreeMap<[u32; 3], Vec<usize>> = Default::default();
    for (t, tet) in mesh.tets.iter().enumerate() {
        for skip in 0..4 {
            let mut f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| tet[i]).collect();
            f.sort_unstable();
            owners.entry([f[0], f[1], f[2]]).or_default().push(t);
        }
    }
    let mut checked = 0usize;
    for (f, ts) in owners.iter().filter(|(_, v)| v.len() == 2) {
        let pts: [[f64; 3]; 3] = f.map(|v| mesh.nodes[v as usize]);
        let e1 = [
            pts[1][0] - pts[0][0],
            pts[1][1] - pts[0][1],
            pts[1][2] - pts[0][2],
        ];
        let e2 = [
            pts[2][0] - pts[0][0],
            pts[2][1] - pts[0][1],
            pts[2][2] - pts[0][2],
        ];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = n.map(|v| v / nn);
        for &(a, b) in &[(0.2, 0.3), (0.6, 0.1), (0.25, 0.5)] {
            let xp: [f64; 3] = std::array::from_fn(|d| pts[0][d] + a * e1[d] + b * e2[d]);
            let vals: Vec<[c64; 3]> = ts
                .iter()
                .map(|&t| {
                    let v: [[f64; 3]; 4] =
                        std::array::from_fn(|i| mesh.nodes[mesh.tets[t][i] as usize]);
                    let (lam, _) = bary_at(&v, xp);
                    space.field_at(&mesh, t, lam, &x)
                })
                .collect();
            for comp in [|z: c64| z.re, |z: c64| z.im] {
                let ua = tangential(vals[0].map(comp), n);
                let ub = tangential(vals[1].map(comp), n);
                for d in 0..3 {
                    assert!((ua[d] - ub[d]).abs() < 1e-12, "face {f:?}: tangential jump");
                }
            }
        }
        checked += 1;
    }
    assert!(checked > 20, "only {checked} interior faces checked");
}

// ---------------------------------------------------------------------------
// The assembled p=2 curl-curl annihilates global P2 gradients
// ---------------------------------------------------------------------------

/// `K·g` for the p=2 operator's `K = A(0)` (no σ, so `A(0) = K`).
fn k_times(op: &DrivenOperator, g_full: &[c64]) -> (Vec<c64>, f64, f64) {
    let a = op.matrix_at(0.0).expect("A(0)");
    let inv = op.interior_to_full();
    let g: Vec<c64> = inv.iter().map(|&f| g_full[f]).collect();
    let mut kg = vec![c64::new(0.0, 0.0); inv.len()];
    let mut row_sum = vec![0.0_f64; inv.len()];
    let a = a.as_ref();
    for j in 0..a.ncols() {
        let rows = a.row_idx_of_col_raw(j);
        let vals = a.val_of_col(j);
        for (&i, &v) in rows.iter().zip(vals.iter()) {
            kg[i] += v * g[j];
            row_sum[i] += v.norm();
        }
    }
    let k_inf = row_sum.iter().copied().fold(0.0, f64::max);
    let g_inf = g.iter().map(|z| z.norm()).fold(0.0, f64::max);
    (kg, k_inf, g_inf)
}

#[test]
fn assembled_p2_curl_curl_annihilates_global_p2_gradients_on_a_tagged_mesh() {
    let mesh = jittered_cube(3, 13);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let walls_z0 = plane_faces(&mesh, 2, 0.0);
    let walls_x1 = plane_faces(&mesh, 0, 1.0);
    let wall_nodes: BTreeSet<u32> = walls_z0
        .iter()
        .chain(&walls_x1)
        .flatten()
        .copied()
        .collect();
    let mask_tagged = space.pec_interior_mask(&mesh, &[walls_z0.as_slice(), walls_x1.as_slice()]);
    let mask_free = vec![true; space.n_dofs()];

    // A random global P2 scalar φ = Σ_v φ_v λ_v + Σ_e c_e λ_a λ_b; its
    // gradient in the hierarchical basis is W_e = φ_b − φ_a, Q_e = c_e.
    // For the tagged case φ vanishes on the wall vertices and wall edges,
    // so ∇φ satisfies the PEC condition and lives in the interior space.
    for (label, mask) in [("free", &mask_free), ("tagged", &mask_tagged)] {
        let mut rng = Lcg(17);
        let tagged = label == "tagged";
        let phi_v: Vec<f64> = (0..mesh.n_nodes())
            .map(|v| {
                let r = rng.next();
                if tagged && wall_nodes.contains(&(v as u32)) {
                    0.0
                } else {
                    r
                }
            })
            .collect();
        let mut g = vec![c64::new(0.0, 0.0); space.n_dofs()];
        for (ge, e) in space.edges().iter().enumerate() {
            let d = space.edge_dofs(ge);
            let c_e = rng.next();
            let on_wall = !mask[d[1] as usize];
            g[d[0] as usize] = c64::new(phi_v[e[1] as usize] - phi_v[e[0] as usize], 0.0);
            g[d[1] as usize] = c64::new(if on_wall { 0.0 } else { c_e }, 0.0);
        }
        if tagged {
            // ∇φ is tangentially zero on the walls: its eliminated entries
            // are exactly zero.
            for (d, &keep) in mask.iter().enumerate() {
                if !keep {
                    assert_eq!(g[d], c64::new(0.0, 0.0), "gradient not PEC-compatible");
                }
            }
        }
        let zero = CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        };
        let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
        let op = DrivenOperator::assemble_with_space::<B>(
            &space,
            &mesh,
            DrivenMaterials::Scalar(&eps),
            None,
            &DrivenBcs {
                pec_interior_mask: mask,
            },
            &[],
            &[],
            DrivenSource::Constant(&zero),
            &device(),
        )
        .expect("p=2 operator");
        let (kg, k_inf, g_inf) = k_times(&op, &g);
        let kg_inf = kg.iter().map(|z| z.norm()).fold(0.0, f64::max);
        let rel = kg_inf / (k_inf * g_inf);
        println!(
            "{label}: ‖K g‖∞ = {kg_inf:.3e}, ‖K‖∞ = {k_inf:.3e}, ‖g‖∞ = {g_inf:.3e}, \
             ratio = {rel:.3e}"
        );
        assert!(k_inf > 1.0 && g_inf > 0.1, "{label}: degenerate fixture");
        assert!(
            rel <= 1e-12,
            "{label}: ‖K g‖∞ / (‖K‖∞ ‖g‖∞) = {rel:.3e} > 1e-12"
        );
    }
}

#[test]
fn prolong_p1_reproduces_the_p1_field_exactly() {
    let mesh = jittered_cube(2, 4);
    let s1 = HcurlSpace::build(&mesh, ElementOrder::P1);
    let s2 = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mut rng = Lcg(8);
    let x1: Vec<c64> = (0..s1.n_dofs())
        .map(|_| c64::new(rng.next(), rng.next()))
        .collect();
    let x2 = s2.prolong_p1(&x1);
    assert_eq!(x2.len(), s2.n_dofs());
    for t in 0..mesh.n_tets() {
        for lam in [[0.25; 4], [0.1, 0.2, 0.3, 0.4], [0.7, 0.1, 0.1, 0.1]] {
            let e1 = s1.field_at(&mesh, t, lam, &x1);
            let e2 = s2.field_at(&mesh, t, lam, &x2);
            let c1 = s1.curl_at(&mesh, t, lam, &x1);
            let c2 = s2.curl_at(&mesh, t, lam, &x2);
            for d in 0..3 {
                assert!((e1[d] - e2[d]).norm() < 1e-12, "tet {t}: field");
                assert!((c1[d] - c2[d]).norm() < 1e-11, "tet {t}: curl");
            }
        }
    }
    assert_eq!(s1.prolong_p1(&x1), x1, "p=1 prolongation is the identity");
}
