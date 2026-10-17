//! Programmatic tetrahedral mesh generators for the electrostatic
//! capacitance oracles (Epic #475).
//!
//! Per the issue's meshing preference — *"programmatic meshing preferred
//! over checked-in `.msh` where feasible"* — the coax and concentric-sphere
//! oracle geometries are generated in-Rust rather than committed as Gmsh
//! fixtures. This also sidesteps the `GmshReader` surface-group gap entirely
//! (the base reader drops surface elements and per-element tags): here the
//! conductor node sets are identified **geometrically** (by radius) at
//! generation time, so no `$PhysicalNames`/surface-element parsing is
//! needed.
//!
//! All generators return a [`TetMesh`] plus the conductor/ground node sets
//! and per-tet region tags needed by
//! [`crate::assembly::electrostatic`]. Tets are emitted with positive signed
//! volume (the assembler uses `|det|`, so orientation is not load-bearing,
//! but positive orientation keeps the fixtures conventional).

use std::f64::consts::PI;

use crate::mesh::TetMesh;

/// A coaxial-cylinder shell fixture: the dielectric annulus between an
/// inner conductor at radius `a` and an outer conductor at radius `b`,
/// extruded a length `length` in `z`. The `z = 0` and `z = length` end
/// faces are left **natural (Neumann)** so the 3-D solve reproduces the 2-D
/// per-unit-length capacitance `C/L = 2πε / ln(b/a)`.
#[derive(Clone, Debug)]
pub struct CoaxFixture {
    /// The volume mesh (annular cylindrical shell).
    pub mesh: TetMesh,
    /// Node indices on the inner conductor surface (`r = a`).
    pub inner: Vec<u32>,
    /// Node indices on the outer conductor surface (`r = b`).
    pub outer: Vec<u32>,
    /// Per-tet region tag (all `0` — homogeneous dielectric; kept for
    /// `build_eps_r` uniformity with the multi-region fixtures).
    pub tet_tags: Vec<i32>,
    /// Boundary triangles of the inner conductor surface (outward oriented,
    /// away from the conductor = radially inward here), for the
    /// surface-flux cross-check.
    pub inner_triangles: Vec<[u32; 3]>,
    /// Axial extent of the fixture.
    pub length: f64,
}

/// Generate a coaxial-shell mesh.
///
/// * `a`, `b` — inner / outer conductor radii (`a < b`).
/// * `length` — axial extent (extruded in `z`).
/// * `n_theta` — azimuthal divisions (≥ 8 for a sane annulus).
/// * `n_r` — radial divisions across the annulus (≥ 2).
/// * `n_z` — axial divisions (≥ 1).
///
/// The `(r, θ, z)` structured grid is split into 6 tets per hex cell.
pub fn coax_shell_mesh(
    a: f64,
    b: f64,
    length: f64,
    n_theta: usize,
    n_r: usize,
    n_z: usize,
) -> CoaxFixture {
    assert!(a > 0.0 && b > a, "require 0 < a < b");
    assert!(n_theta >= 3 && n_r >= 1 && n_z >= 1);

    // Node index: (ir, ith, iz). θ wraps (ith in 0..n_theta), r in 0..=n_r,
    // z in 0..=n_z.
    let nr1 = n_r + 1;
    let nz1 = n_z + 1;
    let node_idx = |ir: usize, ith: usize, iz: usize| -> u32 {
        (ir + (ith % n_theta) * nr1 + iz * nr1 * n_theta) as u32
    };

    let mut nodes: Vec<[f64; 3]> = Vec::with_capacity(nr1 * n_theta * nz1);
    for iz in 0..nz1 {
        let z = length * iz as f64 / n_z as f64;
        for ith in 0..n_theta {
            let theta = 2.0 * PI * ith as f64 / n_theta as f64;
            for ir in 0..nr1 {
                let r = a + (b - a) * ir as f64 / n_r as f64;
                nodes.push([r * theta.cos(), r * theta.sin(), z]);
            }
        }
    }

    let mut tets: Vec<[u32; 4]> = Vec::new();
    for iz in 0..n_z {
        for ith in 0..n_theta {
            for ir in 0..n_r {
                // Hex cell corners (r,θ,z) → 8 nodes.
                let c = [
                    node_idx(ir, ith, iz),
                    node_idx(ir + 1, ith, iz),
                    node_idx(ir + 1, ith + 1, iz),
                    node_idx(ir, ith + 1, iz),
                    node_idx(ir, ith, iz + 1),
                    node_idx(ir + 1, ith, iz + 1),
                    node_idx(ir + 1, ith + 1, iz + 1),
                    node_idx(ir, ith + 1, iz + 1),
                ];
                push_hex_tets(&mut tets, &c, &nodes);
            }
        }
    }

    // Conductor node sets by radius.
    let mut inner = Vec::new();
    let mut outer = Vec::new();
    for iz in 0..nz1 {
        for ith in 0..n_theta {
            inner.push(node_idx(0, ith, iz));
            outer.push(node_idx(n_r, ith, iz));
        }
    }
    inner.sort_unstable();
    inner.dedup();
    outer.sort_unstable();
    outer.dedup();

    // Inner-surface triangles (r = a quad faces split into 2 tris).
    let mut inner_triangles = Vec::new();
    for iz in 0..n_z {
        for ith in 0..n_theta {
            let p00 = node_idx(0, ith, iz);
            let p01 = node_idx(0, ith + 1, iz);
            let p10 = node_idx(0, ith, iz + 1);
            let p11 = node_idx(0, ith + 1, iz + 1);
            inner_triangles.push([p00, p01, p11]);
            inner_triangles.push([p00, p11, p10]);
        }
    }

    let n_tets = tets.len();
    CoaxFixture {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        },
        inner,
        outer,
        tet_tags: vec![0; n_tets],
        inner_triangles,
        length,
    }
}

/// A concentric-sphere shell fixture: the dielectric shell between an inner
/// conductor sphere of radius `a` and an outer conductor sphere of radius
/// `b`. Genuinely 3-D. Analytic `C = 4πε · ab/(b−a)`.
#[derive(Clone, Debug)]
pub struct SphereShellFixture {
    /// The volume mesh (spherical shell).
    pub mesh: TetMesh,
    /// Node indices on the inner conductor sphere (`r = a`).
    pub inner: Vec<u32>,
    /// Node indices on the outer conductor sphere (`r = b`).
    pub outer: Vec<u32>,
    /// Per-tet region tag (all `0` — homogeneous dielectric).
    pub tet_tags: Vec<i32>,
    /// Inner-sphere boundary triangles (for the surface-flux cross-check).
    pub inner_triangles: Vec<[u32; 3]>,
}

/// Generate a concentric-sphere shell mesh by subdividing an icosahedron
/// into a geodesic sphere and radially layering it.
///
/// * `a`, `b` — inner / outer radii (`a < b`).
/// * `subdiv` — icosphere subdivision level (0 = raw icosahedron with 12
///   vertices / 20 faces; each level quadruples the faces). `subdiv = 2`
///   (≈ 320 faces) gives a decent shell for the ≤1% oracle at `n_r = 2..3`.
/// * `n_r` — radial layers across the shell (≥ 2 recommended).
pub fn sphere_shell_mesh(a: f64, b: f64, subdiv: usize, n_r: usize) -> SphereShellFixture {
    assert!(a > 0.0 && b > a, "require 0 < a < b");
    assert!(n_r >= 1);

    let (unit_verts, faces) = icosphere(subdiv);
    let n_surf = unit_verts.len();
    let nr1 = n_r + 1;

    // Layered nodes: layer ir at radius r(ir), node = ir + iv*nr1.
    let node_idx = |ir: usize, iv: usize| -> u32 { (ir + iv * nr1) as u32 };
    let mut nodes: Vec<[f64; 3]> = Vec::with_capacity(nr1 * n_surf);
    for u in unit_verts.iter().take(n_surf) {
        for ir in 0..nr1 {
            let r = a + (b - a) * ir as f64 / n_r as f64;
            nodes.push([r * u[0], r * u[1], r * u[2]]);
        }
    }

    // Each surface triangle × radial layer forms a triangular prism, split
    // into 3 tets.
    let mut tets: Vec<[u32; 4]> = Vec::new();
    for ir in 0..n_r {
        for f in &faces {
            let (v0, v1, v2) = (f[0], f[1], f[2]);
            // Prism between inner layer ir (bottom) and ir+1 (top).
            let b0 = node_idx(ir, v0);
            let b1 = node_idx(ir, v1);
            let b2 = node_idx(ir, v2);
            let t0 = node_idx(ir + 1, v0);
            let t1 = node_idx(ir + 1, v1);
            let t2 = node_idx(ir + 1, v2);
            push_prism_tets(&mut tets, [b0, b1, b2, t0, t1, t2], &nodes);
        }
    }

    let mut inner = Vec::with_capacity(n_surf);
    let mut outer = Vec::with_capacity(n_surf);
    for iv in 0..n_surf {
        inner.push(node_idx(0, iv));
        outer.push(node_idx(n_r, iv));
    }

    let inner_triangles: Vec<[u32; 3]> = faces
        .iter()
        .map(|f| [node_idx(0, f[0]), node_idx(0, f[1]), node_idx(0, f[2])])
        .collect();

    let n_tets = tets.len();
    SphereShellFixture {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        },
        inner,
        outer,
        tet_tags: vec![0; n_tets],
        inner_triangles,
    }
}

/// A two-sphere-in-grounded-box fixture (oracle 3): two conductor spheres
/// of radius `r_sph` centered at `±(sep/2)` along `x`, immersed in a
/// grounded cubic box of half-side `half`. The dielectric is vacuum.
#[derive(Clone, Debug)]
pub struct TwoSphereBoxFixture {
    /// The volume mesh (box minus the two conductor balls).
    pub mesh: TetMesh,
    /// Node indices on sphere A (`x = −sep/2`).
    pub sphere_a: Vec<u32>,
    /// Node indices on sphere B (`x = +sep/2`).
    pub sphere_b: Vec<u32>,
    /// Node indices on the grounded outer box boundary.
    pub ground: Vec<u32>,
    /// Per-tet region tag (all `0`).
    pub tet_tags: Vec<i32>,
}

/// Generate a two-sphere-in-grounded-box mesh.
///
/// The domain is a **single connected** structured tet grid over the whole
/// box `[-half, half]³` (6 tets/hex). Rather than carving the conductor
/// balls out (which would disconnect the mesh and float the conductors),
/// the two conductors are represented as **Dirichlet node sets baked into
/// the box grid**: every grid node within `r_sph` of a sphere center is a
/// conductor node (a "staircase" ball), the outer box walls are ground, and
/// the dielectric is the remaining interior. This keeps the mesh conforming
/// and fully connected so the off-diagonal coupling is real; the staircase
/// discretization of the spheres plus the finite box together set the
/// honest few-% band of oracle 3 (documented in `results.toml`).
///
/// * `r_sph` — conductor sphere radius.
/// * `sep` — center-to-center separation (`> 2 r_sph`), along `x`.
/// * `half` — grounded box half-side (`> sep/2 + r_sph`).
/// * `n_box` — box grid divisions per axis (resolves the staircase spheres;
///   ≥ ~20 for a sane ball).
pub fn two_sphere_box_mesh(r_sph: f64, sep: f64, half: f64, n_box: usize) -> TwoSphereBoxFixture {
    assert!(r_sph > 0.0, "require r_sph > 0");
    assert!(sep > 2.0 * r_sph, "spheres must not overlap: sep > 2 r_sph");
    assert!(half > sep / 2.0 + r_sph, "box must enclose both spheres");
    assert!(n_box >= 4);

    let cx_a = -sep / 2.0;
    let cx_b = sep / 2.0;

    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut tets: Vec<[u32; 4]> = Vec::new();

    let nb1 = n_box + 1;
    let step = 2.0 * half / n_box as f64;
    let box_node = |i: usize, j: usize, k: usize| -> usize { i + j * nb1 + k * nb1 * nb1 };
    for k in 0..nb1 {
        for j in 0..nb1 {
            for i in 0..nb1 {
                nodes.push([
                    -half + i as f64 * step,
                    -half + j as f64 * step,
                    -half + k as f64 * step,
                ]);
            }
        }
    }
    // Every hex cell becomes 6 tets (single connected mesh).
    for k in 0..n_box {
        for j in 0..n_box {
            for i in 0..n_box {
                let c = [
                    box_node(i, j, k) as u32,
                    box_node(i + 1, j, k) as u32,
                    box_node(i + 1, j + 1, k) as u32,
                    box_node(i, j + 1, k) as u32,
                    box_node(i, j, k + 1) as u32,
                    box_node(i + 1, j, k + 1) as u32,
                    box_node(i + 1, j + 1, k + 1) as u32,
                    box_node(i, j + 1, k + 1) as u32,
                ];
                push_hex_tets(&mut tets, &c, &nodes);
            }
        }
    }

    // Conductor node sets = grid nodes inside each staircase ball; ground =
    // box-wall nodes. A node on a wall that also lies inside a ball is a
    // conductor (shouldn't happen for a well-separated fixture).
    let mut sphere_a = Vec::new();
    let mut sphere_b = Vec::new();
    let mut ground = Vec::new();
    for k in 0..nb1 {
        for j in 0..nb1 {
            for i in 0..nb1 {
                let idx = box_node(i, j, k) as u32;
                let p = nodes[idx as usize];
                let da = dist(p, [cx_a, 0.0, 0.0]);
                let db = dist(p, [cx_b, 0.0, 0.0]);
                if da <= r_sph {
                    sphere_a.push(idx);
                } else if db <= r_sph {
                    sphere_b.push(idx);
                } else if i == 0 || i == n_box || j == 0 || j == n_box || k == 0 || k == n_box {
                    ground.push(idx);
                }
            }
        }
    }

    let n_tets = tets.len();
    TwoSphereBoxFixture {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: Default::default(),
        },
        sphere_a,
        sphere_b,
        ground,
        tet_tags: vec![0; n_tets],
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Geometry helpers.
// ─────────────────────────────────────────────────────────────────────────

/// Split a hex cell (8 corner node indices in the standard VTK ordering) into
/// 6 tets sharing the `c[0]→c[6]` diagonal — the same split
/// [`crate::mesh::cube_tet_mesh`] uses. Reorders each tet to positive signed
/// volume using the actual node coordinates.
fn push_hex_tets(tets: &mut Vec<[u32; 4]>, c: &[u32; 8], nodes: &[[f64; 3]]) {
    const SPLIT: [[usize; 4]; 6] = [
        [0, 1, 2, 6],
        [0, 2, 3, 6],
        [0, 3, 7, 6],
        [0, 7, 4, 6],
        [0, 4, 5, 6],
        [0, 5, 1, 6],
    ];
    for s in &SPLIT {
        tets.push(oriented_tet([c[s[0]], c[s[1]], c[s[2]], c[s[3]]], nodes));
    }
}

/// Split a triangular prism (bottom triangle `b0,b1,b2`; top `t0,t1,t2`) into
/// 3 tets. Input order: `[b0, b1, b2, t0, t1, t2]`. Reorients each tet to
/// positive signed volume.
fn push_prism_tets(tets: &mut Vec<[u32; 4]>, p: [u32; 6], nodes: &[[f64; 3]]) {
    let [b0, b1, b2, t0, t1, t2] = p;
    let split = [[b0, b1, b2, t2], [b0, b1, t2, t1], [b0, t1, t2, t0]];
    for s in &split {
        tets.push(oriented_tet(*s, nodes));
    }
}

/// Return the tet with its last two vertices swapped if needed so the signed
/// volume is positive.
fn oriented_tet(t: [u32; 4], nodes: &[[f64; 3]]) -> [u32; 4] {
    let v0 = nodes[t[0] as usize];
    let e1 = sub3(nodes[t[1] as usize], v0);
    let e2 = sub3(nodes[t[2] as usize], v0);
    let e3 = sub3(nodes[t[3] as usize], v0);
    let det = dot3(e1, cross3(e2, e3));
    if det < 0.0 {
        [t[0], t[1], t[3], t[2]]
    } else {
        t
    }
}

/// Build a unit-radius geodesic sphere (icosphere) at the given subdivision
/// level. Returns `(vertices on the unit sphere, triangular faces)`.
fn icosphere(subdiv: usize) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
    // Base icosahedron.
    let t = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut verts: Vec<[f64; 3]> = vec![
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ];
    for v in verts.iter_mut() {
        normalize(v);
    }
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    use std::collections::HashMap;
    for _ in 0..subdiv {
        let mut mid_cache: HashMap<(usize, usize), usize> = HashMap::new();
        let mut new_faces = Vec::with_capacity(faces.len() * 4);
        let mut midpoint = |a: usize, b: usize, verts: &mut Vec<[f64; 3]>| -> usize {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&m) = mid_cache.get(&key) {
                return m;
            }
            let mut mp = [
                (verts[a][0] + verts[b][0]) * 0.5,
                (verts[a][1] + verts[b][1]) * 0.5,
                (verts[a][2] + verts[b][2]) * 0.5,
            ];
            normalize(&mut mp);
            let idx = verts.len();
            verts.push(mp);
            mid_cache.insert(key, idx);
            idx
        };
        for f in &faces {
            let a = midpoint(f[0], f[1], &mut verts);
            let b = midpoint(f[1], f[2], &mut verts);
            let c = midpoint(f[2], f[0], &mut verts);
            new_faces.push([f[0], a, c]);
            new_faces.push([f[1], b, a]);
            new_faces.push([f[2], c, b]);
            new_faces.push([a, b, c]);
        }
        faces = new_faces;
    }
    (verts, faces)
}

#[inline]
fn normalize(v: &mut [f64; 3]) {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v[0] /= n;
    v[1] /= n;
    v[2] /= n;
}
#[inline]
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
#[inline]
fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub3(a, b);
    dot3(d, d).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coax_mesh_has_positive_volume_tets() {
        let f = coax_shell_mesh(1.0, 2.0, 1.0, 16, 3, 2);
        assert!(f.mesh.n_tets() > 0);
        for tet in &f.mesh.tets {
            let v0 = f.mesh.nodes[tet[0] as usize];
            let e1 = sub3(f.mesh.nodes[tet[1] as usize], v0);
            let e2 = sub3(f.mesh.nodes[tet[2] as usize], v0);
            let e3 = sub3(f.mesh.nodes[tet[3] as usize], v0);
            assert!(
                dot3(e1, cross3(e2, e3)) > 0.0,
                "tet has non-positive volume"
            );
        }
        assert!(!f.inner.is_empty() && !f.outer.is_empty());
    }

    #[test]
    fn sphere_shell_radii_are_correct() {
        let (a, b) = (1.0, 2.0);
        let f = sphere_shell_mesh(a, b, 1, 2);
        for &n in &f.inner {
            let p = f.mesh.nodes[n as usize];
            assert!((dot3(p, p).sqrt() - a).abs() < 1e-9);
        }
        for &n in &f.outer {
            let p = f.mesh.nodes[n as usize];
            assert!((dot3(p, p).sqrt() - b).abs() < 1e-9);
        }
        for tet in &f.mesh.tets {
            let v0 = f.mesh.nodes[tet[0] as usize];
            let e1 = sub3(f.mesh.nodes[tet[1] as usize], v0);
            let e2 = sub3(f.mesh.nodes[tet[2] as usize], v0);
            let e3 = sub3(f.mesh.nodes[tet[3] as usize], v0);
            assert!(dot3(e1, cross3(e2, e3)) > 1e-14, "degenerate/negative tet");
        }
    }

    #[test]
    fn icosphere_vertices_on_unit_sphere() {
        let (v, faces) = icosphere(2);
        // Level-2 icosphere: 20·4² = 320 faces, and by Euler's formula for a
        // closed triangulation V = F/2 + 2 = 162 vertices.
        assert_eq!(faces.len(), 320);
        assert_eq!(v.len(), 162);
        for p in &v {
            assert!((dot3(*p, *p).sqrt() - 1.0).abs() < 1e-12);
        }
    }
}
