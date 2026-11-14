//! The Floquet-port slab unit cell shared by the `geode` CLI integration
//! tests `floquet_slab_golden` and `floquet_prom` (issue #1610): the cell
//! `[0, A] × [0, A] × [0, L]`, periodic in x and y, with a dielectric slab
//! (`ε_r = EPS`, `z ∈ (Z1, Z2)`), and the base driven spec over it.
//! Natural units (`c = 1`, `ω = k₀`).
//!
//! Include with `#[path = "support/floquet_cell.rs"] mod floquet_cell;`,
//! alongside `#[path = "support/msh.rs"] mod msh;` — this module writes the
//! cell through the including crate's `msh` module rather than loading
//! `support/msh.rs` a second time (`clippy::duplicate_mod`).
#![allow(dead_code)] // each test binary uses a different subset

use crate::msh;
use serde_json::{Value, json};

pub const A: f64 = 0.1;
pub const L: f64 = 1.0;
pub const K0: f64 = 2.5;
pub const EPS: f64 = 4.0;
pub const Z1: f64 = 0.35;
pub const Z2: f64 = 0.65;

/// Kuhn-split `n[0] × n[1] × n[2]` box of size `[A, A, L]`, the tets with
/// centroid in `(Z1, Z2)` tagged `slab`, the rest `air`; the six walls are
/// tagged `x_lo` … `z_hi`. `x_hi_shift` moves the `x_hi` nodes with odd `j`
/// along `y` by that fraction of the cell height `h_y` — the `x_hi` face
/// then no longer is a translate of `x_lo` (non-matching) unless it is
/// ~1e-9 (snapped).
pub fn box_msh(n: [usize; 3], slab: bool, x_hi_shift: f64) -> String {
    let h = [A / n[0] as f64, A / n[1] as f64, L / n[2] as f64];
    let id = |i: usize, j: usize, k: usize| ((k * (n[1] + 1) + j) * (n[0] + 1) + i) as u32;
    let mut nodes = Vec::new();
    for k in 0..=n[2] {
        for j in 0..=n[1] {
            for i in 0..=n[0] {
                let mut p = [i as f64 * h[0], j as f64 * h[1], k as f64 * h[2]];
                if i == n[0] && j % 2 == 1 {
                    p[1] += x_hi_shift * h[1];
                }
                nodes.push(p);
            }
        }
    }
    let perms = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let (mut air, mut slab_t): (Vec<[u32; 4]>, Vec<[u32; 4]>) = (Vec::new(), Vec::new());
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                for p in perms {
                    let mut c = [i, j, k];
                    let mut t = [id(c[0], c[1], c[2]); 4];
                    for (s, &axis) in p.iter().enumerate() {
                        c[axis] += 1;
                        t[s + 1] = id(c[0], c[1], c[2]);
                    }
                    let pt = |v: u32| nodes[v as usize];
                    let (a, b, c, d) = (pt(t[0]), pt(t[1]), pt(t[2]), pt(t[3]));
                    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let w = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
                    let det = u[0] * (v[1] * w[2] - v[2] * w[1])
                        - u[1] * (v[0] * w[2] - v[2] * w[0])
                        + u[2] * (v[0] * w[1] - v[1] * w[0]);
                    if det < 0.0 {
                        t.swap(2, 3);
                    }
                    let zc = (a[2] + b[2] + c[2] + d[2]) / 4.0;
                    if slab && zc > Z1 && zc < Z2 {
                        slab_t.push(t);
                    } else {
                        air.push(t);
                    }
                }
            }
        }
    }
    let mut walls: [Vec<[u32; 3]>; 6] = Default::default();
    let ijk = |v: u32| {
        let v = v as usize;
        let i = v % (n[0] + 1);
        let j = (v / (n[0] + 1)) % (n[1] + 1);
        let k = v / ((n[0] + 1) * (n[1] + 1));
        [i, j, k]
    };
    for t in air.iter().chain(&slab_t) {
        for skip in 0..4 {
            let f: Vec<u32> = (0..4).filter(|&i| i != skip).map(|i| t[i]).collect();
            let tri = [f[0], f[1], f[2]];
            for axis in 0..3 {
                for (side, idx) in [(0, 0), (1, n[axis])] {
                    if tri.iter().all(|&v| ijk(v)[axis] == idx) {
                        walls[2 * axis + side].push(tri);
                    }
                }
            }
        }
    }
    let names = ["x_lo", "x_hi", "y_lo", "y_hi", "z_lo", "z_hi"];
    let surfaces: Vec<(i32, &str, &[[u32; 3]])> = names
        .iter()
        .zip(&walls)
        .enumerate()
        .map(|(i, (name, tris))| (101 + i as i32, *name, tris.as_slice()))
        .collect();
    let mut volumes: Vec<(i32, &str, &[[u32; 4]])> = vec![(1, "air", air.as_slice())];
    if slab {
        volumes.push((2, "slab", slab_t.as_slice()));
    }
    msh::write_msh_volumes(&nodes, &volumes, &surfaces)
}

/// `f` (Hz) of the natural-unit `k₀` for a 1 m length unit.
pub fn hz(k0: f64) -> f64 {
    k0 * geode_core::constants::C_M_PER_S / (2.0 * std::f64::consts::PI)
}

/// The base spec: slab cell, periodic x / y, Floquet ports on z, `drive`
/// merged in (`incidence` or `scan`).
pub fn spec(drive: Value) -> Value {
    let mut v = json!({
        "schema_version": 1,
        "mesh": {"path": "cell.msh", "length_unit_m": 1.0},
        "materials": [{"physical_group": "slab", "eps_r": [EPS, 0.0]}],
        "boundary_conditions": {"periodic": [
            {"master": "x_lo", "slave": "x_hi"},
            {"master": "y_lo", "slave": "y_hi"}
        ]},
        "floquet_ports": [
            {"physical_group": "z_lo", "n_evanescent": 1},
            {"physical_group": "z_hi", "n_evanescent": 1}
        ],
        "frequencies": {"unit": "hz", "values": [hz(K0)]}
    });
    for (k, x) in drive.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}
