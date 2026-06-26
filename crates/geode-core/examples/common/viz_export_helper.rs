//! Shared `--export-field` helper for the driven/scattering benchmark
//! examples (Epic #276 Phase 2B, issue #287).
//!
//! This module is `#[path]`-included by `mie_sphere.rs`,
//! `spiral_inductor.rs`, and `patch_antenna.rs`. It owns two pieces of
//! example-local logic so the three examples do not each hand-roll
//! them:
//!
//! 1. [`parse_export_field`] — recognise the opt-in
//!    `--export-field <path.vtu>` directive in the example's argv,
//!    matching the by-hand positional-arg style the examples already
//!    use (no new arg-parsing crate).
//! 2. [`edge_field_to_nodes`] — collapse a lowest-order Nédélec
//!    edge-DOF solution (`e_edges`, one complex DOF per global edge in
//!    `mesh.edges()` order) into the per-node `E` vectors
//!    (`[[f64; 3]]`, length `mesh.n_nodes()`) that
//!    [`geode_core::postproc::viz::write_vtu`] consumes.
//!
//! # Sampling choice (v1, intentionally crude)
//!
//! `write_vtu` wants `E` sampled at the mesh *nodes*, but the Whitney
//! 1-form interpolant `E(x) = Σ_e d_e (λ_a ∇λ_b − λ_b ∇λ_a)` is only
//! tangentially continuous across faces — it is multi-valued at a
//! shared node. We evaluate the interpolant at each vertex of every
//! incident tet (barycentric coordinate = the unit vector at that
//! local vertex) and **average** the contributions over the tets that
//! touch the node. This is a debugging visual for ParaView, not a
//! quadrature-accurate reconstruction; the averaging smooths the
//! per-tet discontinuity into a single nodal value.
//!
//! The geometry / DOF-folding / Whitney evaluation mirror the verified
//! `pub(crate)` evaluators in `geode_core::driven::scattering`
//! (`tet_geometry`, `local_dofs`, `eval_field_at_bary`), re-implemented
//! here against the public mesh API because those crate-internal
//! helpers are not visible from an example (a separate crate). Keeping
//! them example-local avoids widening `geode-core`'s public surface for
//! a viz-only need.
//!
//! TODO(viz): higher-order sampling (e.g. quadrature-projected nodal
//! averaging) if the crude per-tet-vertex average proves too noisy for
//! the intended ParaView inspection.

// This module is `#[path]`-included by three examples, but not every
// example exercises every item: `parse_export_field` / `edge_field_to_nodes`
// are used by all three, while the Phase 3C sweep helpers (`SweepSpec`,
// `parse_export_sweep`, `write_pvd`) are only used by `patch_antenna`. The
// unused-in-this-binary items would otherwise trip `-D warnings` dead-code
// in the mie/spiral binaries, so allow it module-wide for this shared
// example helper.
#![allow(dead_code)]

use faer::c64;

use geode_core::mesh::TetMesh;

/// Local edge → (local vertex a, local vertex b), the canonical
/// lowest-order Nédélec edge ordering (`geode_core::mesh::TET_LOCAL_EDGES`).
const LOCAL_EDGES: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

/// Scan `args` (the full process argv) for the opt-in
/// `--export-field <path>` directive and return the requested output
/// path when present.
///
/// The directive is recognised in two equivalent spellings so it slots
/// next to the examples' existing by-hand positional dispatch:
///
/// * `--export-field <path>` (flag + following token), or
/// * the positional pair `export-field <path>`.
///
/// Returns `None` when neither spelling is present, leaving the
/// example's default benchmark behaviour byte-for-byte unchanged.
pub fn parse_export_field(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--export-field" || a == "export-field" {
            return it.next().cloned();
        }
        if let Some(rest) = a.strip_prefix("--export-field=") {
            return Some(rest.to_string());
        }
    }
    None
}

/// A frequency-sweep field-export request parsed from the example argv
/// (Epic #276 Phase 3C, issue #291).
///
/// The sweep writes one `E_<index>.vtu` per swept frequency into
/// [`dir`] plus a ParaView `.pvd` collection ([`write_pvd`]) so
/// `sweep_animate.py` can render the band as an MP4. It is the
/// frequency-domain sibling of [`parse_export_field`]: one driven solve
/// per source frequency `ω`, not a time-domain `E(r, t)`.
pub struct SweepSpec {
    /// Output directory for the `E_<index>.vtu` frames and the `.pvd`.
    pub dir: String,
    /// Sweep start frequency (GHz, inclusive).
    pub f_start_ghz: f64,
    /// Sweep stop frequency (GHz, inclusive).
    pub f_stop_ghz: f64,
    /// Number of swept frequencies (frames). Must be `>= 1`.
    pub n: usize,
}

impl SweepSpec {
    /// The swept frequencies (GHz), evenly spaced over
    /// `[f_start_ghz, f_stop_ghz]` inclusive. A single-point sweep
    /// (`n == 1`) returns just `f_start_ghz`.
    pub fn freqs_ghz(&self) -> Vec<f64> {
        if self.n <= 1 {
            return vec![self.f_start_ghz];
        }
        let step = (self.f_stop_ghz - self.f_start_ghz) / (self.n - 1) as f64;
        (0..self.n)
            .map(|i| self.f_start_ghz + step * i as f64)
            .collect()
    }
}

/// Scan `args` (the full process argv) for the opt-in
/// `--export-sweep <dir>` directive and its companion flags and return
/// the parsed [`SweepSpec`] when present.
///
/// Spellings (matching the by-hand positional style the examples use):
///
/// * `--export-sweep <dir>` (flag + following token), or
/// * `--export-sweep=<dir>`.
///
/// The band/count flags are `--f-start <ghz>`, `--f-stop <ghz>`, and
/// `--n <count>` (each also accepts the `--flag=value` spelling).
/// `--f-start` / `--f-stop` default to the patch S11-band 2.0 / 3.0 GHz
/// and `--n` defaults to 11 when omitted, so `--export-sweep <dir>`
/// alone is a usable invocation.
///
/// Returns `None` when `--export-sweep` is absent, leaving the example's
/// default benchmark behaviour byte-for-byte unchanged.
pub fn parse_export_sweep(args: &[String]) -> Option<SweepSpec> {
    fn flag_value(args: &[String], name: &str) -> Option<String> {
        let mut it = args.iter();
        let eq = format!("{name}=");
        while let Some(a) = it.next() {
            if a == name {
                return it.next().cloned();
            }
            if let Some(rest) = a.strip_prefix(&eq) {
                return Some(rest.to_string());
            }
        }
        None
    }

    let dir = flag_value(args, "--export-sweep")?;
    let f_start_ghz = flag_value(args, "--f-start")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let f_stop_ghz = flag_value(args, "--f-stop")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3.0);
    let n = flag_value(args, "--n")
        .and_then(|s| s.parse().ok())
        .unwrap_or(11usize)
        .max(1);
    Some(SweepSpec {
        dir,
        f_start_ghz,
        f_stop_ghz,
        n,
    })
}

/// Write a ParaView `.pvd` collection mapping each `E_<index>.vtu` frame
/// to a `timestep` (the swept frequency in GHz), so ParaView (and
/// `sweep_animate.py`) treats the frequency sweep as a time-series.
///
/// `frames` is `(timestep, file_name)` pairs where `file_name` is the
/// frame's path **relative to the `.pvd`** (e.g. `E_0000.vtu`) — keeping
/// it relative lets the collection move with its directory. The `.pvd`
/// format is a tiny hand-rolled XML, consistent with the Phase 2A `.vtu`
/// writer (no XML dependency):
///
/// ```xml
/// <?xml version="1.0"?>
/// <VTKFile type="Collection" version="0.1" byte_order="LittleEndian">
///   <Collection>
///     <DataSet timestep="2.0" group="" part="0" file="E_0000.vtu"/>
///     ...
///   </Collection>
/// </VTKFile>
/// ```
pub fn write_pvd(path: &std::path::Path, frames: &[(f64, String)]) -> std::io::Result<()> {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"Collection\" version=\"0.1\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <Collection>\n");
    for (timestep, file) in frames {
        s.push_str(&format!(
            "    <DataSet timestep=\"{timestep}\" group=\"\" part=\"0\" file=\"{file}\"/>\n"
        ));
    }
    s.push_str("  </Collection>\n");
    s.push_str("</VTKFile>\n");
    std::fs::write(path, s)
}

/// Cross product of two 3-vectors.
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Dot product of two 3-vectors.
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `a - b` for 3-vectors.
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Barycentric gradients `∇λ_i` (constant over the tet) for the tet
/// with the given 0-based vertex indices.
///
/// Mirrors `geode_core::driven::scattering::tet_geometry`.
fn tet_grads(mesh: &TetMesh, tet: &[u32; 4]) -> [[f64; 3]; 4] {
    let v = [
        mesh.nodes[tet[0] as usize],
        mesh.nodes[tet[1] as usize],
        mesh.nodes[tet[2] as usize],
        mesh.nodes[tet[3] as usize],
    ];
    let e1 = sub(v[1], v[0]);
    let e2 = sub(v[2], v[0]);
    let e3 = sub(v[3], v[0]);
    let det = dot(e1, cross(e2, e3));
    let inv = if det != 0.0 { 1.0 / det } else { 0.0 };
    let grad1 = cross(e2, e3).map(|x| x * inv);
    let grad2 = cross(e3, e1).map(|x| x * inv);
    let grad3 = cross(e1, e2).map(|x| x * inv);
    let grad0 = [
        -(grad1[0] + grad2[0] + grad3[0]),
        -(grad1[1] + grad2[1] + grad3[1]),
        -(grad1[2] + grad2[2] + grad3[2]),
    ];
    [grad0, grad1, grad2, grad3]
}

/// Average the Whitney edge-DOF field at the mesh nodes.
///
/// `e_edges` is the full-length complex edge-DOF vector (one entry per
/// global edge in `mesh.edges()` order, e.g.
/// `geode_core::driven::DrivenSolution::e_edges`). Returns the per-node
/// real and imaginary `E` vectors, each of length `mesh.n_nodes()`,
/// ready to hand to `geode_core::postproc::viz::write_vtu`.
///
/// See the module docs for the (crude, averaging) sampling choice.
pub fn edge_field_to_nodes(mesh: &TetMesh, e_edges: &[c64]) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let n_nodes = mesh.n_nodes();
    let tet_edges = mesh.tet_edges();

    let mut e_re = vec![[0.0_f64; 3]; n_nodes];
    let mut e_im = vec![[0.0_f64; 3]; n_nodes];
    let mut counts = vec![0_u32; n_nodes];

    for (t, tet) in mesh.tets.iter().enumerate() {
        let grad = tet_grads(mesh, tet);
        // Sign-folded local edge DOFs, in LOCAL_EDGES order.
        let dofs: [c64; 6] = std::array::from_fn(|e| {
            let (idx, sign) = tet_edges[t][e];
            e_edges[idx as usize] * c64::new(sign as f64, 0.0)
        });

        // Evaluate the Whitney interpolant at each of the 4 vertices
        // (barycentric coord = unit vector at that local vertex) and
        // accumulate onto the corresponding global node.
        for local_v in 0..4 {
            let mut lambda = [0.0_f64; 4];
            lambda[local_v] = 1.0;
            let mut e = [c64::new(0.0, 0.0); 3];
            for (slot, &(a, b)) in LOCAL_EDGES.iter().enumerate() {
                let d = dofs[slot];
                for (k, e_k) in e.iter_mut().enumerate() {
                    let w = lambda[a] * grad[b][k] - lambda[b] * grad[a][k];
                    *e_k += d * c64::new(w, 0.0);
                }
            }
            let node = tet[local_v] as usize;
            for k in 0..3 {
                e_re[node][k] += e[k].re;
                e_im[node][k] += e[k].im;
            }
            counts[node] += 1;
        }
    }

    for node in 0..n_nodes {
        if counts[node] > 0 {
            let inv = 1.0 / counts[node] as f64;
            for k in 0..3 {
                e_re[node][k] *= inv;
                e_im[node][k] *= inv;
            }
        }
    }

    (e_re, e_im)
}
