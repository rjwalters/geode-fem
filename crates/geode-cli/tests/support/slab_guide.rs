//! The slab-loaded rectangular waveguide shared by the `geode` CLI
//! integration tests `optimize_loop` and `sensitivity_nport` (issue #1610):
//! a `A × B_DIM × LEN` guide meshed by
//! [`extruded_rect_waveguide_mesh`], written as Gmsh MSH 4.1.
//!
//! Include with `#[path = "support/slab_guide.rs"] mod slab_guide;`,
//! alongside `#[path = "support/msh.rs"] mod msh;` — this module writes the
//! mesh through the including crate's `msh` module rather than loading
//! `support/msh.rs` a second time (`clippy::duplicate_mod`).
#![allow(dead_code)] // each test binary uses a different subset

use std::path::{Path, PathBuf};

use crate::msh::write_msh_volumes as write_msh;
use geode_core::driven::ports::extruded_rect_waveguide_mesh;

pub const A: f64 = 2.0;
pub const B_DIM: f64 = 1.0;
pub const LEN: f64 = 1.2;

/// The `2 × 1 × 1.2` guide (8 × 4 × 6 cells) with a dielectric `slab`
/// filling the middle third (`0.4 < z < 0.8`) between two `guide` (air)
/// ends; `port_in` / `port_out` are the end caps, `walls` the PEC sides.
pub fn write_slab_guide(dir: &Path) -> PathBuf {
    let g = extruded_rect_waveguide_mesh(8, 4, 6, A, B_DIM, LEN);
    let (mut slab, mut air) = (Vec::new(), Vec::new());
    for t in &g.mesh.tets {
        let z = t.iter().map(|&v| g.mesh.nodes[v as usize][2]).sum::<f64>() / 4.0;
        if z > 0.4 && z < 0.8 {
            slab.push(*t);
        } else {
            air.push(*t);
        }
    }
    let msh = write_msh(
        &g.mesh.nodes,
        &[(1, "guide", &air), (2, "slab", &slab)],
        &[
            (11, "port_in", &g.port1_faces),
            (12, "port_out", &g.port2_faces),
            (13, "walls", &g.sidewall_faces),
        ],
    );
    let path = dir.join("slab_guide.msh");
    std::fs::write(&path, msh).unwrap();
    path
}
