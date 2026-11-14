//! The analytic rectangular wave-port builder shared by the
//! `geode-core` integration tests.
//!
//! Nine copies of this construction used to live inline across seven
//! test files (issue #1570). They all ran the same five steps, differing
//! only in which of `(nx, ny, a, b, n_modes, a_inc, medium)` were
//! parameters and which were file-level constants.

use faer::c64;
use geode_core::analytic::waveguide::{rect_tri_mesh, solve_rect_waveguide_modes};
use geode_core::driven::ports::{PortMedium, PortMode, WavePort, map_mode_profile_to_full_mesh};
use geode_core::mesh::TetMesh;

/// Build the lowest `n_modes` analytic TE modes of an `a × b`
/// rectangular cross-section as a [`WavePort`] on the `z = z_plane`
/// face of `mesh`.
///
/// The port mesh is the 2-D [`rect_tri_mesh`] with the same
/// `(nx, ny, a, b)` — its vertex layout matches the 3-D mesh's port
/// face exactly, so the modal eigenvector indexed in the 2-D edge table
/// maps edge-for-edge into the 3-D edge table.
///
/// The 2-D port mesh sits at `z = 0` (it is 2-D); the 3-D port-face
/// triangulation has the same `(x, y)` vertices but with `z = z_plane`.
/// To map, each 2-D node `(x, y)` is shifted to `(x, y, z_plane)` and
/// looked up by **3-D node tag** in `mesh`. Each 2-D edge is then
/// relabeled to 3-D node tags and re-sorted lower-tag-first to match
/// the 3-D edge convention; the true `(lo, hi)`-keyed lookup is
/// [`map_mode_profile_to_full_mesh`]'s job.
///
/// The returned port is a vacuum port — chain
/// [`WavePort::with_medium`] for a filled guide.
///
/// # Panics
/// Panics if a port-face node is not present in `mesh` at `z_plane`, if
/// the 2-D modal solve fails, or if it returns fewer than `n_modes`
/// modes.
#[allow(dead_code)] // some test binaries only use a subset of helpers
pub fn rect_wave_port(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    z_plane: f64,
    [nx, ny]: [usize; 2],
    [a, b]: [f64; 2],
    n_modes: usize,
    a_inc: c64,
) -> WavePort {
    let port_mesh = rect_tri_mesh(nx, ny, a, b);

    // 2-D node tag → 3-D node tag, by location. The tolerance is scaled
    // by the cross-section so it stays relative for non-O(1) guides; it
    // is far below any mesh spacing either way.
    let tol = 1e-9 * a.max(b).max(1.0);
    let three_d_idx_of = |x: f64, y: f64| -> u32 {
        mesh.nodes
            .iter()
            .position(|p| {
                (p[0] - x).abs() < tol && (p[1] - y).abs() < tol && (p[2] - z_plane).abs() < tol
            })
            .expect("port-face node not found in the 3-D mesh") as u32
    };
    let n2d_to_n3d: Vec<u32> = port_mesh
        .nodes
        .iter()
        .map(|p| three_d_idx_of(p[0], p[1]))
        .collect();

    // 2-D port-mesh edges relabeled to 3-D node tags, lower-tag-first.
    let edges_2d_relabeled: Vec<[u32; 2]> = port_mesh
        .edges()
        .iter()
        .map(|e| {
            let (a3, b3) = (n2d_to_n3d[e[0] as usize], n2d_to_n3d[e[1] as usize]);
            if a3 < b3 { [a3, b3] } else { [b3, a3] }
        })
        .collect();

    let modes = solve_rect_waveguide_modes(&port_mesh, a, b, n_modes).expect("2-D modal solve");
    assert_eq!(modes.len(), n_modes, "2-D modal solve returned fewer modes");

    let edges_3d = mesh.edges();
    WavePort {
        faces: faces.to_vec(),
        modes: modes
            .iter()
            .map(|m| PortMode {
                mode: map_mode_profile_to_full_mesh(&edges_2d_relabeled, &m.e_edges, &edges_3d),
                k_c: m.k_c,
                a_inc,
            })
            .collect(),
        medium: PortMedium::VACUUM,
    }
}
