//! Palace-style **uniform lumped port** (Epic #193, issue #202).
//!
//! A lumped port is a rectangular boundary surface Γ_p bridging two
//! conductors — width `w` along the conductors, gap length `l` across
//! them — that carries both the **excitation** that drives a structure
//! and the **resistive termination** `R` that loads it. The port field
//! is assumed uniform across the gap with unit direction `ê` (Palace's
//! "uniform" port), and the lumped resistance maps to the surface
//! impedance
//!
//! ```text
//! Z_s = R · w / l        (units of η₀; natural units η₀ = 1)
//! ```
//!
//! # Boundary condition and weak form
//!
//! With the codebase's `exp(+jωt)` convention (`∇×E = −jωH`, μ₀ = 1)
//! the matched-source port boundary condition on Γ_p (outward normal
//! `n`) is the Robin condition
//!
//! ```text
//! n × (∇×E) + (jω/Z_s) n × (n × E) = −(2jω/Z_s) E_inc,
//! E_inc = (V_inc / l) ê,
//! ```
//!
//! which is exact for a superposition of an incident wave (drive,
//! entering the domain) and an outgoing wave (reflection), both with
//! wave impedance `Z_s`. Setting `Z_s = 1`, `V_inc = 0` recovers the
//! first-order Silver-Müller absorber (see `silvermuller.rs`, whose
//! derivation this module follows). In the curl-curl weak form the
//! boundary term `−∮ (n×v)·(∇×E)` then contributes
//!
//! - **Termination** — a port admittance term on the system matrix:
//!   `A(ω) += (jω/Z_s) S_p`, with the real-symmetric tangential surface
//!   mass `S_p[i,j] = ∮_{Γ_p} (n×N_i)·(n×N_j) dS = ∮ N_i·N_j dS`
//!   (BAC-CAB rank reduction on flat faces, as in `silvermuller.rs`).
//!   Real `R` keeps `A(ω)ᵀ = A(ω)` — the complex-symmetry invariant
//!   (PR #55) is preserved.
//! - **Excitation** — a surface-current-like RHS:
//!   `b_i += (2jω/Z_s)(V_inc/l) ∮_{Γ_p} N_i · ê dS`, the boundary
//!   analogue of the volumetric `b_i = jω ∫ N_i · J dV` drive.
//!
//! # Port voltage / current bookkeeping
//!
//! The circuit quantities needed to read an input impedance off the
//! field solution (full `Z(ω) → L/R/Q` extraction is issue #203):
//!
//! ```text
//! V  = (1/w) ∮_{Γ_p} E · ê dS          (gap line integral of E,
//!                                       averaged across the width)
//! I  = (2 V_inc − V) / R               (admittance relation of the
//!                                       Thevenin port: source 2·V_inc
//!                                       behind R)
//! Z_in = V / I
//! ```
//!
//! With these conventions a structure that presents a matched load
//! (`Z_in = R`) absorbs the drive completely (`V = V_inc`), and the
//! analytic transmission-line oracle (PEC-shorted parallel-plate line
//! of length `d`, characteristic impedance `Z₀ = l/w`) reads
//! `Z_in = j Z₀ tan(ωd)` — see `tests/lumped_port.rs`.
//!
//! # Discretization
//!
//! Same first-order Whitney triangle kernel as the Silver-Müller
//! surface matrix: the mass integrand `N_i·N_j` is degree-2 in
//! barycentric coordinates and is integrated with the 3-point
//! edge-midpoint rule (Hammer-Stroud, degree-2 exact); the flux
//! integrand `N_i·ê` is degree-1 with the closed form
//! `∫_T N_e dA = (area/3)(∇λ_b − ∇λ_a)`. Edge orientation signs follow
//! the lower-tag-first global convention of `nedelec_assembly`.
//!
//! Assembly here is host-side `f64` (like
//! [`crate::assembly::surface::assemble_silver_muller_surface`]); the port
//! terms are frequency-independent real matrices/vectors scaled by
//! `jω/Z_s` at solve time inside [`crate::driven`].

use faer::c64;

use crate::elements::whitney::{
    self, TRI_LOCAL_EDGES, dot3, edge_lookup, face_geometry, scale3, sub3,
};
use crate::mesh::TetMesh;

/// Palace-style uniform lumped port specification.
///
/// The port surface is given as an explicit triangle list (gmsh
/// physical-group mapping arrives with Phase 3 meshing). All triangles
/// must be faces of the volume mesh on its boundary, and `e_hat` must
/// be a **unit** vector tangential to the port plane, pointing across
/// the gap (from one conductor to the other).
#[derive(Debug, Clone)]
pub struct LumpedPort<'a> {
    /// Port surface triangles: `[n][3]` 0-based node indices into
    /// `mesh.nodes`. Winding order does not matter (the tangential
    /// kernel is orientation-free, like the Silver-Müller one).
    pub faces: &'a [[u32; 3]],
    /// Uniform unit field direction `ê` across the gap (tangential to
    /// the port faces).
    pub e_hat: [f64; 3],
    /// Lumped port resistance `R` in natural units (units of η₀).
    pub resistance: f64,
    /// Port width `w` (extent perpendicular to `ê`, along the
    /// conductors).
    pub width: f64,
    /// Gap length `l` (extent along `ê`, between the conductors).
    pub length: f64,
    /// Incident (drive) voltage `V_inc` across the gap. Zero makes the
    /// port a passive resistive termination.
    pub v_inc: c64,
}

impl LumpedPort<'_> {
    /// Surface impedance of the uniform port: `Z_s = R · w / l`.
    pub fn surface_impedance(&self) -> f64 {
        self.resistance * self.width / self.length
    }
}

/// Assemble the **tangential surface mass** of a port surface as signed
/// global triplets:
///
/// ```text
/// S_p[i, j] = ∮_{Γ_p} (n × N_i) · (n × N_j) dS = ∮_{Γ_p} N_i · N_j dS
/// ```
///
/// Returns `(row, col, value)` triplets over global edge indices
/// (duplicate entries to be summed by the caller, e.g. faer's
/// `try_new_from_triplets`). The assembled matrix is real symmetric, so
/// scaling by `jω/Z_s` preserves the complex-symmetry invariant
/// `A(ω)ᵀ = A(ω)`.
///
/// Same kernel as
/// [`crate::assembly::surface::assemble_surface_mass_triplets`] — both are
/// thin delegates to the shared `whitney` module (3-point
/// edge-midpoint quadrature, degree-2 exact; issue #208), so the two
/// entry points produce bit-identical triplet streams. The unit tests
/// cross-validate this path against the dense Silver-Müller assembly as
/// a regression on the unified kernel.
///
/// # Panics
///
/// Panics if a face references an edge absent from `edges` (i.e. the
/// triangles are not faces of the volume mesh).
pub fn assemble_port_surface_mass(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    edges: &[[u32; 2]],
) -> Vec<(usize, usize, f64)> {
    whitney::assemble_surface_mass_triplets(mesh, faces, edges)
}

/// Assemble the **port flux vector**
///
/// ```text
/// f_i = ∮_{Γ_p} N_i · ê dS
/// ```
///
/// as a full-length `[n_edges]` real vector (non-zero only on port-face
/// edges). This single vector serves both port roles:
///
/// - **excitation**: `b_i += (2jω/Z_s)(V_inc/l) f_i`,
/// - **voltage readout**: `V = (1/w) Σ_i f_i E_i` (see
///   [`port_voltage`]),
///
/// which is the discrete reciprocity structure that makes the port
/// drive/measure pair adjoint-consistent.
///
/// The integrand is degree-1 in barycentric coordinates, integrated
/// with the closed form `∫_T N_e dA = (area/3)(∇λ_b − ∇λ_a)`. Only the
/// in-plane component of `e_hat` contributes (the Whitney traces are
/// tangential); `e_hat` should be a unit vector tangential to Γ_p.
///
/// # Panics
///
/// Panics if a face references an edge absent from `edges`.
pub fn assemble_port_flux(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    e_hat: [f64; 3],
    edges: &[[u32; 2]],
) -> Vec<f64> {
    let lookup = edge_lookup(edges);
    let mut flux = vec![0.0_f64; edges.len()];

    for tri in faces {
        let v: [[f64; 3]; 3] = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let geo = face_geometry(tri, &v, &lookup);

        for (k, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
            let (gi, si) = geo.edge_info[k];
            // ∫_T N_e dA = (area/3)(∇λ_lb − ∇λ_la).
            let integral = scale3(
                sub3(geo.grad_lambda[lb], geo.grad_lambda[la]),
                geo.area / 3.0,
            );
            flux[gi as usize] += dot3(integral, e_hat) * (si as f64);
        }
    }

    flux
}

/// Port voltage from the field projection:
///
/// ```text
/// V = (1/w) ∮_{Γ_p} E · ê dS = (1/w) Σ_i f_i E_i,
/// ```
///
/// the line integral of `E` along `ê` across the gap, averaged over the
/// port width (for the uniform port the line integral is the same on
/// every line, up to discretization).
///
/// `e_edges` is the full-length edge-DOF vector in `mesh.edges()`
/// order, e.g. [`crate::driven::solve::DrivenSolution::e_edges`]; `edges` is
/// that same edge table.
pub fn port_voltage(
    mesh: &TetMesh,
    port: &LumpedPort<'_>,
    edges: &[[u32; 2]],
    e_edges: &[c64],
) -> c64 {
    let flux = assemble_port_flux(mesh, port.faces, port.e_hat, edges);
    let mut v = c64::new(0.0, 0.0);
    for (f, e) in flux.iter().zip(e_edges.iter()) {
        v += *e * *f;
    }
    v * (1.0 / port.width)
}

/// Port current from the admittance relation of the Thevenin port
/// (ideal source `2·V_inc` behind the lumped resistance `R`):
///
/// ```text
/// I = (2 V_inc − V) / R.
/// ```
///
/// `I` is the current delivered into the structure; for a matched load
/// (`V = V_inc`) it is `V_inc / R`.
pub fn port_current(port: &LumpedPort<'_>, v_port: c64) -> c64 {
    (port.v_inc * 2.0 - v_port) * (1.0 / port.resistance)
}

/// Input impedance seen at the port: `Z_in = V / I` with `V` from
/// [`port_voltage`] and `I` from [`port_current`].
///
/// This is the structure's input impedance **excluding** the port's own
/// resistance `R` (the source impedance) — the quantity the
/// transmission-line oracle compares against `j Z₀ tan(ωd)`. The
/// full `Z(ω) → L/R/Q/S` extraction API and the assembly-reusing
/// frequency-sweep driver live in [`crate::driven::extraction`] (issue #203).
pub fn port_input_impedance(
    mesh: &TetMesh,
    port: &LumpedPort<'_>,
    edges: &[[u32; 2]],
    e_edges: &[c64],
) -> c64 {
    let v = port_voltage(mesh, port, edges, e_edges);
    let i = port_current(port, v);
    v / i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::surface::assemble_silver_muller_surface;
    use crate::mesh::{TetMesh, cube_tet_mesh};
    use std::collections::BTreeMap;

    fn unit_triangle_mesh() -> (TetMesh, Vec<[u32; 3]>) {
        let nodes = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let tets = vec![[0u32, 1, 2, 3]];
        let mesh = TetMesh {
            nodes,
            tets,
            physical_groups: BTreeMap::new(),
        };
        let faces = vec![[0u32, 1, 2]];
        (mesh, faces)
    }

    /// The triplet-form port surface mass must agree with the dense
    /// Silver-Müller surface kernel (independent implementation of the
    /// same integral) on a multi-face boundary patch.
    #[test]
    fn port_mass_matches_silver_muller_kernel() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        // Port patch: every boundary face on z = 0.
        let faces: Vec<[u32; 3]> = mesh
            .faces()
            .into_iter()
            .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-12))
            .collect();
        assert!(!faces.is_empty());

        let tags = vec![1_i32; faces.len()];
        let dense = assemble_silver_muller_surface(&mesh, &faces, &tags, 1, &edges);

        let n = edges.len();
        let mut from_triplets = vec![0.0_f64; n * n];
        for (r, c, v) in assemble_port_surface_mass(&mesh, &faces, &edges) {
            from_triplets[r * n + c] += v;
        }

        let mut max_diff = 0.0_f64;
        for r in 0..n {
            for c in 0..n {
                max_diff = max_diff.max((from_triplets[r * n + c] - dense[(r, c)]).abs());
            }
        }
        assert!(
            max_diff < 1e-14,
            "port mass disagrees with Silver-Müller kernel: {max_diff}"
        );
    }

    /// The two public triplet entry points must produce **bit-identical**
    /// triplet streams (same face order, same duplicate-unsummed
    /// convention) — issue #208 acceptance criterion: both delegate to
    /// the single kernel in `whitney`.
    #[test]
    fn port_mass_triplets_bit_identical_to_silver_muller_triplets() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces: Vec<[u32; 3]> = mesh
            .faces()
            .into_iter()
            .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-12))
            .collect();
        assert!(!faces.is_empty());

        let port = assemble_port_surface_mass(&mesh, &faces, &edges);
        let sm = crate::assembly::surface::assemble_surface_mass_triplets(&mesh, &faces, &edges);
        assert_eq!(port.len(), sm.len());
        for (a, b) in port.iter().zip(sm.iter()) {
            assert_eq!(a.0, b.0);
            assert_eq!(a.1, b.1);
            assert_eq!(a.2.to_bits(), b.2.to_bits(), "triplet values differ");
        }
    }

    /// The assembled port surface mass must be symmetric (the iω-scaled
    /// admittance term inherits A(ω)ᵀ = A(ω) from this).
    #[test]
    fn port_mass_is_symmetric() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces: Vec<[u32; 3]> = mesh
            .faces()
            .into_iter()
            .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-12))
            .collect();
        let n = edges.len();
        let mut s = vec![0.0_f64; n * n];
        for (r, c, v) in assemble_port_surface_mass(&mesh, &faces, &edges) {
            s[r * n + c] += v;
        }
        let mut max_asym = 0.0_f64;
        for r in 0..n {
            for c in 0..n {
                max_asym = max_asym.max((s[r * n + c] - s[c * n + r]).abs());
            }
        }
        assert!(max_asym < 1e-14, "S_p not symmetric: {max_asym}");
    }

    /// Closed-form flux on the unit right triangle: with
    /// `ê = x̂`, `∫_T N_01 · x̂ dA = ∫_T (1 − y) dA = 1/3`,
    /// `∫_T N_02 · x̂ dA = ∫_T y dA = 1/6` and
    /// `∫_T N_12 · x̂ dA = ∫_T (−y) dA = −1/6`
    /// (Whitney traces from the silvermuller.rs analytic test).
    #[test]
    fn flux_matches_analytic_on_unit_triangle() {
        let (mesh, faces) = unit_triangle_mesh();
        let edges = mesh.edges();
        let flux = assemble_port_flux(&mesh, &faces, [1.0, 0.0, 0.0], &edges);

        let idx = |a: u32, b: u32| edges.iter().position(|e| e == &[a, b]).unwrap();
        let tol = 1e-14;
        assert!((flux[idx(0, 1)] - 1.0 / 3.0).abs() < tol);
        assert!((flux[idx(0, 2)] - 1.0 / 6.0).abs() < tol);
        assert!((flux[idx(1, 2)] + 1.0 / 6.0).abs() < tol);
        // Off-face edges carry no flux.
        assert_eq!(flux[idx(0, 3)], 0.0);
        assert_eq!(flux[idx(1, 3)], 0.0);
        assert_eq!(flux[idx(2, 3)], 0.0);
    }

    /// A uniform tangential field `E = ê` interpolated on the port
    /// edges must read back the exact gap voltage: `V = l` (so a port
    /// with `length = l` sees `V/l = 1`). Whitney elements reproduce
    /// constants exactly, so this is a round-trip identity.
    #[test]
    fn uniform_field_voltage_roundtrip() {
        let mesh = cube_tet_mesh(2, 1.0);
        let edges = mesh.edges();
        let faces: Vec<[u32; 3]> = mesh
            .faces()
            .into_iter()
            .filter(|f| f.iter().all(|&n| mesh.nodes[n as usize][2].abs() < 1e-12))
            .collect();
        // Edge DOFs of the constant field E = ŷ: e_i = ∫_edge ŷ · dl
        // = (y_b − y_a) for global edge a → b.
        let e_edges: Vec<c64> = edges
            .iter()
            .map(|e| {
                let dy = mesh.nodes[e[1] as usize][1] - mesh.nodes[e[0] as usize][1];
                c64::new(dy, 0.0)
            })
            .collect();
        let port = LumpedPort {
            faces: &faces,
            e_hat: [0.0, 1.0, 0.0],
            resistance: 1.0,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(0.0, 0.0),
        };
        let v = port_voltage(&mesh, &port, &edges, &e_edges);
        assert!(
            (v - c64::new(1.0, 0.0)).norm() < 1e-13,
            "uniform-field voltage readback: got {v}, want 1"
        );
    }

    /// Surface impedance mapping `Z_s = R·w/l`.
    #[test]
    fn surface_impedance_mapping() {
        let port = LumpedPort {
            faces: &[],
            e_hat: [0.0, 1.0, 0.0],
            resistance: 50.0,
            width: 2.0,
            length: 0.5,
            v_inc: c64::new(0.0, 0.0),
        };
        assert_eq!(port.surface_impedance(), 200.0);
    }

    /// Matched-load circuit identities: V = V_inc ⇒ I = V_inc/R and
    /// Z_in = R.
    #[test]
    fn matched_load_circuit_identities() {
        let port = LumpedPort {
            faces: &[],
            e_hat: [0.0, 1.0, 0.0],
            resistance: 3.0,
            width: 1.0,
            length: 1.0,
            v_inc: c64::new(2.0, 0.0),
        };
        let v = port.v_inc; // matched: no reflection
        let i = port_current(&port, v);
        assert!((i - v / 3.0).norm() < 1e-15);
        let z_in = v / i;
        assert!((z_in - c64::new(3.0, 0.0)).norm() < 1e-13);
    }
}
