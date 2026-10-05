//! Trace compatibility of the p=2 surface kernel (issue #857, Epic #836
//! Phase 1b — the mandatory unit gate of the phase).
//!
//! 1. **Tet restriction ≡ trace kernel.** On every boundary face of a
//!    jittered mesh, the tangential part of every one of the owning tet's 20
//!    p=2 basis functions equals the trace kernel function carrying the same
//!    **global** DOF (and the 12 off-face functions have zero tangential
//!    trace), at random face points, to round-off.
//! 2. **Trace kernel ≡ `tri_nedelec2_local`.** The 3-D surface mass of a
//!    triangle in ascending vertex order equals the 2-D p=2 Nédélec mass of
//!    the same triangle laid flat in its own plane.
//! 3. **Global assembly.** For a random DOF vector, `xᵀ S x` from the
//!    assembled triplets equals `∫_Γ |E_t|² dS` with `E` evaluated by
//!    `HcurlSpace::field_at` in the owning tets; the flux functional equals
//!    `∫_Γ E · ê dS` the same way.
//! 4. **Hierarchy.** A p=1 field injected by `prolong_p1` has the same surface
//!    energy and port flux under the p=2 kernel as under the p=1 Whitney
//!    kernel.
//! 5. **Constants.** A uniform field `E = ŷ` reads back the exact port
//!    voltage `V = l` at p=2.

use faer::c64;
use geode_core::analytic::waveguide::tri_nedelec2_local;
use geode_core::assembly::hcurl_space::{HcurlSpace, TetOrientation};
use geode_core::assembly::surface::assemble_surface_mass_triplets;
use geode_core::assembly::surface_p2::{
    TRI_NEDELEC2_TRACE_DOFS, assemble_p2_port_flux, assemble_p2_surface_mass_triplets,
    p2_trace_dofs, tri_nedelec2_surface_mass, tri_nedelec2_trace_shapes, tri_surface_gradients,
};
use geode_core::driven::ports::assemble_port_flux;
use geode_core::elements::ElementOrder;
use geode_core::elements::nedelec_p2::{tet_barycentric_gradients, tet_nedelec2_shapes};
use geode_core::mesh::{TET_LOCAL_FACES, TetMesh, cube_tet_mesh};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// `cube_tet_mesh(n)` with interior nodes jittered and face-interior
/// boundary nodes jittered **within** their face plane (still flat walls).
fn jittered(n: usize, seed: u64) -> TetMesh {
    let mut mesh = cube_tet_mesh(n, 1.0);
    let h = 1.0 / n as f64;
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        let free: Vec<usize> = (0..3)
            .filter(|&d| p[d] > 1e-12 && p[d] < 1.0 - 1e-12)
            .collect();
        if free.len() >= 2 {
            for d in free {
                p[d] += 0.15 * h * (2.0 * rng.next() - 1.0);
            }
        }
    }
    mesh
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
fn unit_normal(v: &[[f64; 3]; 3]) -> [f64; 3] {
    let n = cross(sub(v[1], v[0]), sub(v[2], v[0]));
    let l = dot(n, n).sqrt();
    n.map(|x| x / l)
}
fn tangential(v: [f64; 3], n: [f64; 3]) -> [f64; 3] {
    let vn = dot(v, n);
    [v[0] - vn * n[0], v[1] - vn * n[1], v[2] - vn * n[2]]
}

/// Every boundary face with its owning tet and the face's local slot.
fn boundary_faces_with_owner(mesh: &TetMesh) -> Vec<([u32; 3], usize)> {
    let bset: std::collections::HashSet<[u32; 3]> = mesh.boundary_faces().into_iter().collect();
    let mut out = Vec::new();
    for (t, tet) in mesh.tets.iter().enumerate() {
        for lf in TET_LOCAL_FACES.iter() {
            let mut tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
            tri.sort_unstable();
            if bset.contains(&tri) {
                out.push((tri, t));
            }
        }
    }
    out
}

/// Natural-order tet barycentrics of the point with ascending-face
/// barycentrics `mu` on face `tri` (sorted) of tet `t`.
fn tet_bary_on_face(mesh: &TetMesh, t: usize, tri: &[u32; 3], mu: [f64; 3]) -> [f64; 4] {
    std::array::from_fn(|i| {
        tri.iter()
            .position(|&g| g == mesh.tets[t][i])
            .map_or(0.0, |k| mu[k])
    })
}

#[test]
fn tet_restriction_equals_trace_kernel_on_every_boundary_face() {
    let mesh = jittered(3, 7);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let mut rng = Lcg(11);
    let mut worst = 0.0_f64;
    let mut worst_off = 0.0_f64;
    let faces = boundary_faces_with_owner(&mesh);
    assert_eq!(faces.len(), 6 * 2 * 9);
    for (tri, t) in &faces {
        let (dofs, s) = p2_trace_dofs(&space, tri).expect("boundary face is a mesh face");
        assert_eq!(&s, tri);
        let v: [[f64; 3]; 3] = std::array::from_fn(|i| mesh.nodes[s[i] as usize]);
        let n = unit_normal(&v);
        let (gs, _) = tri_surface_gradients(&v);
        // The tet basis on sorted coords.
        let coords = space.tet_local_coords(&mesh, *t);
        let (grad, _) = tet_barycentric_gradients(&coords);
        let perm = match space.tet_orientation(*t) {
            TetOrientation::AscendingPerm(p) => p,
            TetOrientation::EdgeSigns(_) => unreachable!(),
        };
        let tet_dofs = space.tet_dofs(*t);
        for _ in 0..5 {
            let (a, b) = (rng.next(), rng.next());
            let (a, b) = if a + b > 1.0 {
                (1.0 - a, 1.0 - b)
            } else {
                (a, b)
            };
            let mu = [1.0 - a - b, a, b];
            let nat = tet_bary_on_face(&mesh, *t, &s, mu);
            let lam: [f64; 4] = std::array::from_fn(|i| nat[perm[i]]);
            let (nt, _) = tet_nedelec2_shapes(&lam, &grad);
            let tr = tri_nedelec2_trace_shapes(&mu, &gs);
            for (i, &gd) in tet_dofs.iter().enumerate() {
                let tan = tangential(nt[i], n);
                match dofs.iter().position(|&d| d == gd) {
                    Some(k) => {
                        let e = sub(tan, tr[k]);
                        worst = worst.max(dot(e, e).sqrt());
                        // The trace kernel is itself tangential.
                        assert!(dot(tr[k], n).abs() < 1e-12);
                    }
                    None => worst_off = worst_off.max(dot(tan, tan).sqrt()),
                }
            }
        }
    }
    println!(
        "trace identity: max |tet tangential − trace| = {worst:.2e}, \
         max off-face tangential trace = {worst_off:.2e}"
    );
    assert!(worst < 1e-12, "tet restriction ≠ trace kernel: {worst:.3e}");
    assert!(
        worst_off < 1e-12,
        "off-face function leaks a trace: {worst_off:.3e}"
    );
}

#[test]
fn surface_mass_equals_flat_tri_nedelec2_mass() {
    let mut rng = Lcg(3);
    let mut worst = 0.0_f64;
    for _ in 0..20 {
        let v: [[f64; 3]; 3] = std::array::from_fn(|_| std::array::from_fn(|_| rng.next()));
        let s3 = tri_nedelec2_surface_mass(&v);
        // Lay the triangle flat in an orthonormal in-plane frame.
        let e1 = sub(v[1], v[0]);
        let e2 = sub(v[2], v[0]);
        let n = unit_normal(&v);
        let u = e1.map(|x| x / dot(e1, e1).sqrt());
        let w = cross(n, u);
        let c2 = [
            [0.0, 0.0],
            [dot(e1, u), dot(e1, w)],
            [dot(e2, u), dot(e2, w)],
        ];
        let (_k2, m2, _area) = tri_nedelec2_local(&c2);
        let scale = (0..8).map(|i| s3[i][i].abs()).fold(0.0, f64::max);
        for i in 0..TRI_NEDELEC2_TRACE_DOFS {
            for j in 0..TRI_NEDELEC2_TRACE_DOFS {
                worst = worst.max((s3[i][j] - m2[i][j]).abs() / scale);
            }
        }
    }
    println!("3-D trace mass vs tri_nedelec2_local mass: max rel diff = {worst:.2e}");
    assert!(worst < 1e-12, "trace mass ≠ tri_nedelec2 mass: {worst:.3e}");
}

/// The degree-4 Duffy rule on a triangle (Gauss–Legendre 4×4), used here
/// as an independent quadrature for the global checks.
fn tri_rule() -> Vec<([f64; 3], f64)> {
    let x = [
        0.069_431_844_202_973_7,
        0.330_009_478_207_571_9,
        0.669_990_521_792_428_1,
        0.930_568_155_797_026_3,
    ];
    let w = [
        0.173_927_422_568_726_9,
        0.326_072_577_431_273_1,
        0.326_072_577_431_273_1,
        0.173_927_422_568_726_9,
    ];
    let mut out = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            let a = x[i];
            let b = x[j] * (1.0 - a);
            // ∫_T f = 2|T| ∫∫ f (1−a) da db  → fraction weight 2·w_i w_j (1−a).
            out.push(([1.0 - a - b, a, b], 2.0 * w[i] * w[j] * (1.0 - a)));
        }
    }
    out
}

#[test]
fn assembled_surface_mass_and_flux_match_field_integrals() {
    let mesh = jittered(3, 21);
    let space = HcurlSpace::build(&mesh, ElementOrder::P2);
    let faces = boundary_faces_with_owner(&mesh);
    let tris: Vec<[u32; 3]> = faces.iter().map(|f| f.0).collect();
    let mut rng = Lcg(5);
    let x: Vec<c64> = (0..space.n_dofs())
        .map(|_| c64::new(rng.next() - 0.5, rng.next() - 0.5))
        .collect();
    let e_hat = [0.3, -0.5, 0.81_f64.sqrt()];

    // xᵀ S x (unconjugated) and fᵀ x from the assembled kernels.
    let trip = assemble_p2_surface_mass_triplets(&space, &mesh, &tris).unwrap();
    let mut xsx = c64::new(0.0, 0.0);
    for &(r, c, v) in &trip {
        xsx += x[r] * x[c] * v;
    }
    let flux = assemble_p2_port_flux(&space, &mesh, &tris, e_hat).unwrap();
    let fx: c64 = flux.iter().zip(&x).map(|(f, xi)| *xi * *f).sum();

    // The same integrals from `field_at` in the owning tets.
    let rule = tri_rule();
    let mut e2 = c64::new(0.0, 0.0);
    let mut ef = c64::new(0.0, 0.0);
    for (tri, t) in &faces {
        let v: [[f64; 3]; 3] = std::array::from_fn(|i| mesh.nodes[tri[i] as usize]);
        let n = unit_normal(&v);
        let area = 0.5 * dot(cross(sub(v[1], v[0]), sub(v[2], v[0])), n);
        for (mu, w) in &rule {
            let bary = tet_bary_on_face(&mesh, *t, tri, *mu);
            let e = space.field_at(&mesh, *t, bary, &x);
            let en = e[0] * n[0] + e[1] * n[1] + e[2] * n[2];
            let et: [c64; 3] = std::array::from_fn(|d| e[d] - en * n[d]);
            e2 += (et[0] * et[0] + et[1] * et[1] + et[2] * et[2]) * (w * area);
            // ê's normal part never contributes: use the tangential field.
            ef += (et[0] * e_hat[0] + et[1] * e_hat[1] + et[2] * e_hat[2]) * (w * area);
        }
    }
    let rel_s = (xsx - e2).norm() / e2.norm();
    let rel_f = (fx - ef).norm() / ef.norm();
    println!("global: xᵀSx vs ∫|E_t|² rel = {rel_s:.2e}; fᵀx vs ∫E_t·ê rel = {rel_f:.2e}");
    assert!(rel_s < 1e-12, "assembled S ≠ ∫|E_t|²: {rel_s:.3e}");
    assert!(rel_f < 1e-12, "assembled flux ≠ ∫E_t·ê: {rel_f:.3e}");

    // Symmetry of the assembled global S.
    let mut dense = std::collections::HashMap::<(usize, usize), f64>::new();
    for &(r, c, v) in &trip {
        *dense.entry((r, c)).or_default() += v;
    }
    let asym = dense
        .iter()
        .map(|(&(r, c), &v)| (v - dense.get(&(c, r)).copied().unwrap_or(0.0)).abs())
        .fold(0.0, f64::max);
    assert!(asym < 1e-15, "S not symmetric: {asym:.3e}");
}

#[test]
fn prolonged_p1_field_has_the_p1_surface_energy_and_flux() {
    let mesh = jittered(3, 9);
    let p2 = HcurlSpace::build(&mesh, ElementOrder::P2);
    let edges = mesh.edges();
    let tris: Vec<[u32; 3]> = mesh.boundary_faces();
    let mut rng = Lcg(13);
    let x1: Vec<c64> = (0..edges.len())
        .map(|_| c64::new(rng.next() - 0.5, rng.next() - 0.5))
        .collect();
    let x2 = p2.prolong_p1(&x1);
    let quad = |trip: &[(usize, usize, f64)], x: &[c64]| -> c64 {
        trip.iter().map(|&(r, c, v)| x[r] * x[c] * v).sum()
    };
    let s1 = quad(&assemble_surface_mass_triplets(&mesh, &tris, &edges), &x1);
    let s2 = quad(
        &assemble_p2_surface_mass_triplets(&p2, &mesh, &tris).unwrap(),
        &x2,
    );
    let e_hat = [0.0, 0.6, 0.8];
    let f1: c64 = assemble_port_flux(&mesh, &tris, e_hat, &edges)
        .iter()
        .zip(&x1)
        .map(|(f, x)| *x * *f)
        .sum();
    let f2: c64 = assemble_p2_port_flux(&p2, &mesh, &tris, e_hat)
        .unwrap()
        .iter()
        .zip(&x2)
        .map(|(f, x)| *x * *f)
        .sum();
    let (rs, rf) = ((s1 - s2).norm() / s1.norm(), (f1 - f2).norm() / f1.norm());
    println!("hierarchy: surface energy rel {rs:.2e}, flux rel {rf:.2e}");
    assert!(rs < 1e-12 && rf < 1e-12, "p=1 ⊂ p=2 broken on the trace");
}

#[test]
fn uniform_field_reads_back_the_gap_voltage_at_p2() {
    let mesh = jittered(4, 2);
    let p2 = HcurlSpace::build(&mesh, ElementOrder::P2);
    // E = ŷ: Whitney DOFs e_i = y_b − y_a, injected into p=2 (exact).
    let x1: Vec<c64> = mesh
        .edges()
        .iter()
        .map(|e| {
            c64::new(
                mesh.nodes[e[1] as usize][1] - mesh.nodes[e[0] as usize][1],
                0.0,
            )
        })
        .collect();
    let x2 = p2.prolong_p1(&x1);
    let port: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-12))
        .collect();
    let flux = assemble_p2_port_flux(&p2, &mesh, &port, [0.0, 1.0, 0.0]).unwrap();
    // V = (1/w) ∮ E·ê dS with w = 1 over the unit face: exactly 1 = l.
    let v: c64 = flux.iter().zip(&x2).map(|(f, x)| *x * *f).sum();
    assert!((v - c64::new(1.0, 0.0)).norm() < 1e-13, "V = {v}");
}

#[test]
fn non_face_triangles_are_rejected() {
    let mesh = cube_tet_mesh(2, 1.0);
    let p2 = HcurlSpace::build(&mesh, ElementOrder::P2);
    // Corner-to-corner triangle spanning several cells: not a mesh face.
    let n = mesh.nodes.len() as u32;
    let bad = [0u32, 2, n - 1];
    assert_eq!(
        assemble_p2_surface_mass_triplets(&p2, &mesh, &[bad]).unwrap_err(),
        bad
    );
    assert_eq!(
        assemble_p2_port_flux(&p2, &mesh, &[bad], [1.0, 0.0, 0.0]).unwrap_err(),
        bad
    );
    assert!(p2_trace_dofs(&p2, &[0, 0, 1]).is_none());
}
