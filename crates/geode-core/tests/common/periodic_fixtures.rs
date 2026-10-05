//! Shared fixtures for the periodic-BC goldens (issue #839): structured box
//! meshes, a deliberately non-translation-symmetric 5-tet split, random node
//! renumbering, and node jitter.
//!
//! Pulled in with `#[path = "common/periodic_fixtures.rs"] mod fixtures;`.
#![allow(dead_code)] // each test binary uses a subset

use std::collections::BTreeMap;

use geode_core::mesh::TetMesh;

/// `[0, a] × [0, b] × [0, c]` with `n = [nx, ny, nz]` hexes per axis, each
/// split into the 6 right-handed tets of `cube_tet_mesh` (so opposite faces
/// match under translation). Lexicographic numbering.
pub fn box_tet_mesh(n: [usize; 3], size: [f64; 3]) -> TetMesh {
    let [nx, ny, nz] = n;
    let h = [
        size[0] / nx as f64,
        size[1] / ny as f64,
        size[2] / nz as f64,
    ];
    let idx = |i: usize, j: usize, k: usize| (i + j * (nx + 1) + k * (nx + 1) * (ny + 1)) as u32;
    let mut nodes = Vec::new();
    for k in 0..=nz {
        for j in 0..=ny {
            for i in 0..=nx {
                nodes.push([i as f64 * h[0], j as f64 * h[1], k as f64 * h[2]]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let c = hex(&idx, i, j, k);
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }
    TetMesh {
        nodes,
        tets,
        physical_groups: BTreeMap::new(),
    }
}

/// The 5-tet checkerboard split of an `n³` cube of side `side`. With `n`
/// **odd** the x = 0 and x = side faces are triangulated with crossed
/// diagonals (the faces' nodes match under translation, their triangles do
/// not): the non-conforming fixture of golden 1(c).
pub fn cube_tet_mesh_5(n: usize, side: f64) -> TetMesh {
    let h = side / n as f64;
    let idx = |i: usize, j: usize, k: usize| (i + j * (n + 1) + k * (n + 1) * (n + 1)) as u32;
    let mut nodes = Vec::new();
    for k in 0..=n {
        for j in 0..=n {
            for i in 0..=n {
                nodes.push([i as f64 * h, j as f64 * h, k as f64 * h]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let c = hex(&idx, i, j, k);
                let local: [[usize; 4]; 5] = if (i + j + k) % 2 == 0 {
                    [
                        [1, 3, 4, 6],
                        [0, 1, 3, 4],
                        [2, 1, 3, 6],
                        [5, 1, 4, 6],
                        [7, 3, 4, 6],
                    ]
                } else {
                    [
                        [0, 2, 5, 7],
                        [1, 0, 2, 5],
                        [3, 0, 2, 7],
                        [4, 0, 5, 7],
                        [6, 2, 5, 7],
                    ]
                };
                for l in local {
                    tets.push(l.map(|v| c[v]));
                }
            }
        }
    }
    let mut mesh = TetMesh {
        nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };
    orient_positive(&mut mesh);
    mesh
}

fn hex(idx: &impl Fn(usize, usize, usize) -> u32, i: usize, j: usize, k: usize) -> [u32; 8] {
    [
        idx(i, j, k),
        idx(i + 1, j, k),
        idx(i + 1, j + 1, k),
        idx(i, j + 1, k),
        idx(i, j, k + 1),
        idx(i + 1, j, k + 1),
        idx(i + 1, j + 1, k + 1),
        idx(i, j + 1, k + 1),
    ]
}

/// Signed volume of tet `t`.
pub fn signed_volume(mesh: &TetMesh, t: usize) -> f64 {
    let p = |i: usize| mesh.nodes[mesh.tets[t][i] as usize];
    let s = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (a, b, c) = (s(p(1), p(0)), s(p(2), p(0)), s(p(3), p(0)));
    (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]))
        / 6.0
}

/// Swap two vertices of every negatively oriented tet.
pub fn orient_positive(mesh: &mut TetMesh) {
    for t in 0..mesh.n_tets() {
        if signed_volume(mesh, t) < 0.0 {
            mesh.tets[t].swap(2, 3);
        }
    }
}

/// Deterministic xorshift64* generator.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `[0, 1)`.
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Apply a random node permutation (Fisher–Yates with `seed`): node `i`
/// becomes node `perm[i]`. Geometry and connectivity are unchanged, but the
/// global (low→high) edge orientations are scrambled, so periodic edge
/// pairs get reversed relative orientations.
pub fn renumber(mesh: &TetMesh, seed: u64) -> TetMesh {
    let n = mesh.n_nodes();
    let mut perm: Vec<u32> = (0..n as u32).collect();
    let mut rng = Rng::new(seed);
    for i in (1..n).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        perm.swap(i, j);
    }
    let mut nodes = vec![[0.0; 3]; n];
    for (i, p) in mesh.nodes.iter().enumerate() {
        nodes[perm[i] as usize] = *p;
    }
    let tets = mesh
        .tets
        .iter()
        .map(|t| t.map(|v| perm[v as usize]))
        .collect();
    TetMesh {
        nodes,
        tets,
        physical_groups: mesh.physical_groups.clone(),
    }
}

/// Move every node with `select(x)` by a random vector of length `amount`
/// within the plane orthogonal to `axis` (so it stays on its boundary face).
pub fn jitter_in_plane(
    mesh: &mut TetMesh,
    axis: usize,
    amount: f64,
    seed: u64,
    select: impl Fn([f64; 3]) -> bool,
) -> usize {
    let mut rng = Rng::new(seed);
    let mut n = 0;
    let lo = mesh.nodes.iter().fold([f64::INFINITY; 3], |a, p| {
        [a[0].min(p[0]), a[1].min(p[1]), a[2].min(p[2])]
    });
    let hi = mesh.nodes.iter().fold([f64::NEG_INFINITY; 3], |a, p| {
        [a[0].max(p[0]), a[1].max(p[1]), a[2].max(p[2])]
    });
    for p in mesh.nodes.iter_mut() {
        if !select(*p) {
            continue;
        }
        let theta = 2.0 * std::f64::consts::PI * rng.uniform();
        let (u, v) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        // Keep nodes on the box's edges / corners where they are (moving
        // them would leave the adjacent faces).
        let on_edge = |d: usize, x: f64| (x - lo[d]).abs() < 1e-12 || (x - hi[d]).abs() < 1e-12;
        if on_edge(u, p[u]) || on_edge(v, p[v]) {
            continue;
        }
        p[u] += amount * theta.cos();
        p[v] += amount * theta.sin();
        n += 1;
    }
    n
}

/// `(b, c)` box face triangles helper: indices of mesh nodes on the plane
/// `x[axis] = value`.
pub fn nodes_on_plane(mesh: &TetMesh, axis: usize, value: f64) -> Vec<usize> {
    (0..mesh.n_nodes())
        .filter(|&i| (mesh.nodes[i][axis] - value).abs() < 1e-9)
        .collect()
}
