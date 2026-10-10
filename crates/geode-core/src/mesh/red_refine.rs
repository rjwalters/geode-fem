//! **Uniform red refinement** of a tet mesh: every tet split into 8, every
//! edge halved (issue #1036, the discretization check on the transmon
//! `C_Σ`).
//!
//! Each tet `(v0, v1, v2, v3)` gets its six edge midpoints. The four corner
//! children are `(v_i, m_ij, m_ik, m_il)`. The inner octahedron is cut into
//! four tets along its **shortest diagonal** (Bey's choice, which keeps the
//! children in a bounded number of similarity classes). Each child is then
//! ordered so its signed volume has the parent's sign. Midpoints are shared
//! through a global edge map, so the fine mesh is conforming.
//!
//! The fine mesh is the coarse mesh with `h` halved. Its P1 space contains
//! the coarse P1 space: a coarse P1 function, linearly interpolated at the
//! midpoints, is unchanged. For an energy-minimum quantity such as a
//! capacitance with fixed (or floating) conductor potentials, refinement
//! therefore can only lower the computed value, provided every coarse
//! Dirichlet set is lifted consistently. [`RedRefinement::lift_sheet_nodes`]
//! does that for sheet conductors: it adds the midpoint of every edge of the
//! conductor's surface triangles.
//!
//! Per-tet data (for example `ε_r`) is inherited from the parent through
//! [`RedRefinement::lift_per_tet`].

use std::collections::BTreeMap;

use super::TetMesh;

/// One uniform red refinement of a [`TetMesh`] (see the
/// [module docs](self)).
#[derive(Clone, Debug)]
pub struct RedRefinement {
    /// The fine mesh: the coarse nodes (same indices) followed by one
    /// midpoint per coarse edge, and `8 × n_tets` tets.
    pub mesh: TetMesh,
    /// `parent[t]` is the coarse tet that fine tet `t` came from.
    pub parent: Vec<usize>,
    /// Number of fine tets whose signed volume differs in sign from their
    /// parent's (0 for a valid refinement; kept as a check).
    pub n_flipped: usize,
    midpoints: BTreeMap<(u32, u32), u32>,
}

fn edge_key(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn signed_vol6(nodes: &[[f64; 3]], t: &[u32; 4]) -> f64 {
    let p = |k: usize| nodes[t[k] as usize];
    let (a, b, c) = (sub(p(1), p(0)), sub(p(2), p(0)), sub(p(3), p(0)));
    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0])
}

fn dist2(nodes: &[[f64; 3]], a: u32, b: u32) -> f64 {
    let d = sub(nodes[a as usize], nodes[b as usize]);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Refine `mesh` once, uniformly: 8 children per tet (see the
/// [module docs](self)).
pub fn red_refine(mesh: &TetMesh) -> RedRefinement {
    let mut nodes = mesh.nodes.clone();
    let mut midpoints: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut tets = Vec::with_capacity(8 * mesh.n_tets());
    let mut parent = Vec::with_capacity(8 * mesh.n_tets());
    let mut n_flipped = 0;
    for (t, q) in mesh.tets.iter().enumerate() {
        let mut mid = |a: u32, b: u32| -> u32 {
            *midpoints.entry(edge_key(a, b)).or_insert_with(|| {
                let (pa, pb) = (mesh.nodes[a as usize], mesh.nodes[b as usize]);
                nodes.push([
                    0.5 * (pa[0] + pb[0]),
                    0.5 * (pa[1] + pb[1]),
                    0.5 * (pa[2] + pb[2]),
                ]);
                (nodes.len() - 1) as u32
            })
        };
        let m01 = mid(q[0], q[1]);
        let m02 = mid(q[0], q[2]);
        let m03 = mid(q[0], q[3]);
        let m12 = mid(q[1], q[2]);
        let m13 = mid(q[1], q[3]);
        let m23 = mid(q[2], q[3]);
        let mut kids = vec![
            [q[0], m01, m02, m03],
            [m01, q[1], m12, m13],
            [m02, m12, q[2], m23],
            [m03, m13, m23, q[3]],
        ];
        // The octahedron's three diagonals join opposite-edge midpoints.
        let diagonals = [(m01, m23), (m02, m13), (m03, m12)];
        let k = (0..3)
            .min_by(|&i, &j| {
                dist2(&nodes, diagonals[i].0, diagonals[i].1).total_cmp(&dist2(
                    &nodes,
                    diagonals[j].0,
                    diagonals[j].1,
                ))
            })
            .unwrap();
        let (dp, dq) = diagonals[k];
        let others: Vec<(u32, u32)> = (0..3).filter(|&i| i != k).map(|i| diagonals[i]).collect();
        // The four equatorial vertices in cyclic order around the diagonal.
        let ring = [others[0].0, others[1].0, others[0].1, others[1].1];
        for i in 0..4 {
            kids.push([dp, dq, ring[i], ring[(i + 1) % 4]]);
        }
        let s0 = signed_vol6(&mesh.nodes, q);
        for mut c in kids {
            if signed_vol6(&nodes, &c) * s0 < 0.0 {
                c.swap(2, 3);
            }
            if signed_vol6(&nodes, &c) * s0 <= 0.0 {
                n_flipped += 1;
            }
            tets.push(c);
            parent.push(t);
        }
    }
    RedRefinement {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: mesh.physical_groups.clone(),
        },
        parent,
        n_flipped,
        midpoints,
    }
}

impl RedRefinement {
    /// The fine node at the midpoint of coarse edge `(a, b)`, if that edge
    /// exists in the coarse mesh.
    pub fn midpoint(&self, a: u32, b: u32) -> Option<u32> {
        self.midpoints.get(&edge_key(a, b)).copied()
    }

    /// Inherit per-tet data from the parent tet.
    pub fn lift_per_tet<T: Clone>(&self, coarse: &[T]) -> Vec<T> {
        self.parent.iter().map(|&p| coarse[p].clone()).collect()
    }

    /// Lift a sheet conductor's node set: `nodes` plus the midpoint of every
    /// edge of its surface `triangles` (sorted, deduplicated).
    ///
    /// # Panics
    ///
    /// If a triangle edge is not an edge of the coarse mesh.
    pub fn lift_sheet_nodes(&self, nodes: &[u32], triangles: &[[u32; 3]]) -> Vec<u32> {
        let mut out = nodes.to_vec();
        for t in triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                out.push(
                    self.midpoint(a, b)
                        .unwrap_or_else(|| panic!("triangle edge ({a}, {b}) is not a mesh edge")),
                );
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;
    use std::collections::BTreeMap;

    fn vol(nodes: &[[f64; 3]], t: &[u32; 4]) -> f64 {
        signed_vol6(nodes, t) / 6.0
    }

    #[test]
    fn red_refine_splits_each_tet_into_eight_conforming_children() {
        let mut coarse = cube_tet_mesh(3, 1.0);
        // Perturb the interior so the children are not all similar.
        for (i, p) in coarse.nodes.iter_mut().enumerate() {
            if p.iter().all(|&x| x > 1e-9 && x < 1.0 - 1e-9) {
                p[0] += 0.03 * ((i % 5) as f64 - 2.0) / 2.0;
                p[2] -= 0.02 * ((i % 3) as f64 - 1.0);
            }
        }
        let mut edges = std::collections::BTreeSet::new();
        for t in &coarse.tets {
            for i in 0..4 {
                for j in i + 1..4 {
                    edges.insert(edge_key(t[i], t[j]));
                }
            }
        }
        let r = red_refine(&coarse);
        assert_eq!(r.mesh.n_tets(), 8 * coarse.n_tets());
        assert_eq!(r.mesh.n_nodes(), coarse.n_nodes() + edges.len());
        assert_eq!(r.n_flipped, 0);
        assert_eq!(&r.mesh.nodes[..coarse.n_nodes()], &coarse.nodes[..]);
        // Children tile their parent: same sign, volumes sum to the parent.
        let mut sum = vec![0.0_f64; coarse.n_tets()];
        for (t, c) in r.mesh.tets.iter().enumerate() {
            let v = vol(&r.mesh.nodes, c);
            let vp = vol(&coarse.nodes, &coarse.tets[r.parent[t]]);
            assert!(v * vp > 0.0, "child {t} has the wrong orientation");
            sum[r.parent[t]] += v;
        }
        for (t, s) in sum.iter().enumerate() {
            let vp = vol(&coarse.nodes, &coarse.tets[t]);
            assert!((s - vp).abs() <= 1e-13 * vp.abs(), "tet {t}");
        }
        // Conforming: every fine face is shared by at most two tets, and the
        // boundary face count is four times the coarse one.
        let faces = |m: &TetMesh| {
            let mut f: BTreeMap<[u32; 3], usize> = BTreeMap::new();
            for t in &m.tets {
                for skip in 0..4 {
                    let mut k: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
                    k.sort_unstable();
                    *f.entry([k[0], k[1], k[2]]).or_insert(0) += 1;
                }
            }
            f
        };
        let (fc, ff) = (faces(&coarse), faces(&r.mesh));
        assert!(ff.values().all(|&n| n <= 2));
        let nb = |f: &BTreeMap<[u32; 3], usize>| f.values().filter(|&&n| n == 1).count();
        assert_eq!(nb(&ff), 4 * nb(&fc));
        // Per-tet data is inherited.
        let tags: Vec<usize> = (0..coarse.n_tets()).collect();
        assert_eq!(r.lift_per_tet(&tags), r.parent);
    }

    #[test]
    fn lift_sheet_nodes_adds_every_triangle_edge_midpoint() {
        let coarse = cube_tet_mesh(2, 1.0);
        let r = red_refine(&coarse);
        // The z = 0 face as a sheet: its boundary triangles.
        let on = |n: u32| coarse.nodes[n as usize][2].abs() < 1e-12;
        let mut tris = Vec::new();
        for t in &coarse.tets {
            for skip in 0..4 {
                let f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
                if f.iter().all(|&n| on(n)) {
                    tris.push([f[0], f[1], f[2]]);
                }
            }
        }
        let nodes: Vec<u32> = (0..coarse.n_nodes() as u32).filter(|&n| on(n)).collect();
        let lifted = r.lift_sheet_nodes(&nodes, &tris);
        let want: Vec<u32> = (0..r.mesh.n_nodes() as u32)
            .filter(|&n| r.mesh.nodes[n as usize][2].abs() < 1e-12)
            .collect();
        assert_eq!(lifted, want, "exactly the fine nodes on the sheet");
    }
}
