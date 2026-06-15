//! Wave (modal) port boundary condition + S-parameter extraction
//! (Epic #234 wave-port, Phase 2, issue #236).
//!
//! Where a *lumped* port ([`crate::lumped_port`]) imposes a uniform
//! voltage/current relation across a gap (the Palace Thévenin
//! formulation), a **wave port** projects onto a true waveguide modal
//! field on a port plane:
//!
//! 1. The 2-D transverse modal eigensolver
//!    ([`crate::waveguide_modes::solve_rect_waveguide_modes`]) on the
//!    port cross-section produces a modal field profile `e_t(x,y)` plus
//!    its cutoff `k_c`. The propagation constant at angular frequency
//!    `ω` is `β(ω) = +√(ω²/c² − k_c²)` (real positive, propagating) or
//!    `β(ω) = −j·√(k_c² − ω²/c²)` (negative imaginary, evanescent — the
//!    outgoing-wave branch under the `+jωt` time convention, issue
//!    #254). See [`WavePort::beta`] and
//!    [`crate::waveguide_modes::beta_outgoing`] for the canonical sign
//!    convention.
//! 2. On the 3-D port face Γ_p the modal Robin / radiation BC adds a
//!    rank-1 modal contribution to the curl-curl system:
//!
//!    ```text
//!    A(ω) += jβ * (f_m ⊗ f_m),       f_m = S_p · e_m   (full-edge vector)
//!    b(ω) += 2jβ · a_inc · f_m       (only for the driven port)
//!    ```
//!
//!    where `S_p` is the port-face tangential surface mass (the same
//!    real-symmetric matrix the Silver-Müller / lumped-port path uses,
//!    [`crate::silvermuller::assemble_surface_mass_triplets`]) and the
//!    modal eigenvector is **`S_p`-orthonormalized**:
//!    `e_m^T S_p e_m = 1`.  Equivalently `f_m^T e_m = 1`.
//!
//!    `jβ * (f_m ⊗ f_m)` is the discrete analog of the modal admittance
//!    surface term `(jω/Z_TE)` for a TE-mode wave port (`1/Z_TE = β/ωμ`,
//!    so the `(jω) Y` factor reduces to `jβ` per mode). The matched
//!    incident wave with amplitude `a_inc` drives the structure with
//!    `2jβ a_inc` so that an ideal matched termination absorbs the wave
//!    completely (the same `2·V_inc` doubling that the lumped-port
//!    Thévenin formulation uses).
//!
//! 3. **Modal projection** (`waveguide_mode_reduce` of the L4 tracker
//!    #5) reads each port's modal amplitude from a driven solution:
//!
//!    ```text
//!    a_m = f_m^T · E    (since e_m^T S_p e_m = 1)
//!    ```
//!
//! 4. **Wave-port S-parameters**: per excitation (port `j` driven at
//!    `a_inc = 1`, all other ports terminated at their matched
//!    admittance), read back each port's modal amplitude `a_k`. Subtract
//!    the incident self-term on the diagonal:
//!
//!    ```text
//!    S_kj = (a_k − a_inc δ_kj) / a_inc.
//!    ```
//!
//!    Reuses the multi-RHS factor-once machinery (`FactoredDrivenOperator`)
//!    so an N-port S-matrix at fixed ω costs one factorization + N
//!    back-substitutions.
//!
//! # Sign convention
//!
//! `exp(+jωt)` time convention, consistent with the rest of the
//! codebase. Propagating mode forward (away from the structure):
//! `e_t(x,y) exp(−jβz)`. The incident wave on a port at the +z end is
//! `exp(+jβz)` (traveling toward the structure); the BC enforces the
//! outgoing Sommerfeld term `−jβ` on the reflected wave (the `+jβ`
//! coefficient on the system matrix comes from the time-derivative sign
//! of the radiation impedance term — see Jin, *The Finite Element Method
//! in Electromagnetics*, §8.4).

use faer::c64;

use crate::driven::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, FactoredDrivenOperator,
    SurfaceImpedanceBc,
};
use crate::lumped_port::LumpedPort;
use crate::silvermuller::assemble_surface_mass_triplets;
use crate::TetMesh;

/// One wave (modal) port: a 3-D port-face triangulation plus the
/// pre-computed modal field profile on that face's Whitney edges, plus
/// the cutoff `k_c` of the mode (the propagation constant
/// `β(ω) = √(ω²/c² − k_c²)` is evaluated per frequency).
///
/// The mode's `e_edges` slot indexes into the **3-D mesh edge table**
/// (`mesh.edges()`), with zeros off the port face. Callers that solved
/// the modal eigenproblem on a stand-alone 2-D cross-section mesh build
/// this profile by mapping the 2-D edge indices to their corresponding
/// 3-D edge indices — the helper
/// [`map_mode_profile_to_full_mesh`] does that mapping for a port-face
/// triangle list.
#[derive(Debug, Clone)]
pub struct WavePort {
    /// Port surface triangles (0-based node indices into `mesh.nodes`).
    /// Each must be a boundary face of the volume mesh.
    pub faces: Vec<[u32; 3]>,
    /// Modal eigenvector over the **3-D mesh edge table**
    /// (`mesh.edges()`), with `e_m^T S_p e_m = 1`. Off-port edges
    /// carry exact zeros.
    pub mode: Vec<f64>,
    /// Cutoff wavenumber `k_c` of the mode (rad / length).
    pub k_c: f64,
    /// Incident modal amplitude `a_inc` for this excitation. Set to
    /// `0` for a passive matched termination, non-zero for a driven
    /// port.
    pub a_inc: c64,
}

impl WavePort {
    /// Propagation constant at angular frequency `omega` (natural units,
    /// `c = 1`): `β² = ω² − k_c²`. Returns a complex β under the
    /// **outgoing-wave** branch convention (`exp(+jωt)` time convention):
    ///
    /// - Propagating (`ω > k_c`): `β = +√(ω² − k_c²)`, real positive.
    /// - Evanescent (`ω < k_c`): `β = −j·√(k_c² − ω²)`,
    ///   `Im(β) < 0` so that `exp(−jβz)` decays for `z > 0`.
    ///
    /// The evanescent branch was fixed in issue #254 (latent bug
    /// flagged in PR #245): the previous default complex `sqrt` branch
    /// gave `Im(β) > 0`, a non-physical growing solution.
    pub fn beta(&self, omega: f64) -> c64 {
        crate::waveguide_modes::beta_outgoing(omega, 1.0, self.k_c)
    }
}

/// Map a 2-D modal profile (eigenvector over the **2-D port-mesh edge
/// table**) onto the **3-D mesh edge table** by matching the port-face
/// triangle list edge-by-edge.
///
/// For each face triangle:
/// - look up its three local edges in the 2-D port mesh's edge table
///   (the same `(lo, hi)` lower-tag-first convention),
/// - look up the same `(lo, hi)` pair in the 3-D mesh's edge table,
/// - copy the 2-D eigenvector value into the 3-D slot.
///
/// **Caller invariant**: the 2-D port-mesh node tags and the 3-D
/// mesh's port-face node tags must agree (i.e. the 2-D port-mesh is the
/// literal triangulation of the port face, with the same node indices).
/// The straight-section / discontinuity fixtures built by
/// [`extruded_rect_waveguide_mesh`] satisfy this by construction.
///
/// # Panics
///
/// Panics if a 2-D port-mesh edge does not appear in the 3-D mesh's
/// edge table.
pub fn map_mode_profile_to_full_mesh(
    port_edges_2d: &[[u32; 2]],
    mode_2d: &[f64],
    mesh_3d_edges: &[[u32; 2]],
) -> Vec<f64> {
    use std::collections::HashMap;
    let mut lookup: HashMap<(u32, u32), usize> = HashMap::with_capacity(mesh_3d_edges.len());
    for (idx, e) in mesh_3d_edges.iter().enumerate() {
        lookup.insert((e[0], e[1]), idx);
    }
    let mut out = vec![0.0_f64; mesh_3d_edges.len()];
    for (idx_2d, e_2d) in port_edges_2d.iter().enumerate() {
        let key = (e_2d[0], e_2d[1]);
        let idx_3d = lookup
            .get(&key)
            .copied()
            .expect("port edge missing from 3-D mesh edge table");
        out[idx_3d] = mode_2d[idx_2d];
    }
    out
}

/// Assemble `f_m = S_p · e_m`, the full-length port flux of a modal
/// field — the workhorse vector behind both the wave-port BC system
/// term `+jβ · f_m ⊗ f_m`, the drive `+2jβ a_inc · f_m`, and the
/// modal-amplitude readout `a_m = f_mᵀ E`.
///
/// `S_p` is the port-face tangential surface mass
/// ([`assemble_surface_mass_triplets`]); the result is a real
/// `[n_edges]` vector supported on the port-face edges.
fn assemble_modal_flux(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    mode: &[f64],
    edges: &[[u32; 2]],
) -> Vec<f64> {
    assert_eq!(
        mode.len(),
        edges.len(),
        "modal profile length {} must match edge count {}",
        mode.len(),
        edges.len()
    );
    let triplets = assemble_surface_mass_triplets(mesh, faces, edges);
    let mut flux = vec![0.0_f64; edges.len()];
    for (r, c, v) in triplets {
        flux[r] += v * mode[c];
    }
    flux
}

/// Modal projection (`waveguide_mode_reduce`, L4 tracker #5): the
/// per-mode complex amplitude `a_m = f_m^T · E`, where `E` is a driven
/// solution's full-length edge vector and `f_m = S_p · e_m` the modal
/// flux of port `p`.
///
/// With `e_m^T S_p e_m = 1` (the wave-port modal solver's normalization),
/// `a_m` is the pure modal coefficient: an incident wave of amplitude
/// `a_inc` produces `a_m ≈ a_inc` on a matched termination.
pub fn waveguide_mode_reduce(
    mesh: &TetMesh,
    port: &WavePort,
    edges: &[[u32; 2]],
    e_edges: &[c64],
) -> c64 {
    assert_eq!(
        e_edges.len(),
        edges.len(),
        "edge vector length {} must match edge count {}",
        e_edges.len(),
        edges.len()
    );
    let flux = assemble_modal_flux(mesh, &port.faces, &port.mode, edges);
    let mut a = c64::new(0.0, 0.0);
    for (f, e) in flux.iter().zip(e_edges.iter()) {
        a += *e * *f;
    }
    a
}

/// One frequency point of an N-port **wave-port** S-parameter sweep
/// ([`solve_wave_port_sweep`]).
///
/// Mirrors [`crate::extraction::SParameterSweepPoint`] in shape but
/// without the impedance matrix (wave ports don't have a Thévenin
/// V/I — the modal amplitude is the natural circuit quantity).
#[derive(Debug, Clone)]
pub struct WavePortSweepPoint {
    /// Frequency `ω ≡ k₀` (natural units).
    pub omega: f64,
    /// Worst (largest) direct-solve relative residual over the N
    /// per-excitation solves at this frequency.
    pub residual_rel: f64,
    /// Row-major `n × n` complex S-matrix:
    /// `s[k*n + j] = (a_k − a_inc_j · δ_kj) / a_inc_j` where `a_k` is
    /// the modal amplitude of port `k` read off the driven solution when
    /// port `j` was excited at amplitude `a_inc_j`.
    pub s: Vec<c64>,
    /// Per-port modal `β(ω)` at this frequency (`(β_re, 0)` propagating,
    /// `(0, β_im)` evanescent).
    pub beta: Vec<c64>,
}

/// N-port **wave-port** S-parameter sweep (issue #236):
///
/// - Assemble the volume operator once (`DrivenOperator::assemble`)
///   with no lumped ports and no Leontovich surfaces; the wave-port
///   modal terms `+jβ · f_m ⊗ f_m` and the per-excitation drive
///   `+2jβ a_inc · f_m` are folded in per frequency, since β depends
///   on ω. Per ω the `A(ω)` is built from the cached K/M/C plus a
///   dense per-port rank-1 correction, factored once, and N
///   excitations are back-substituted (one per driven port).
/// - Each excitation: drive port `j` at its baked `a_inc`, treat all
///   other ports as passive matched terminations (their rank-1
///   admittance term stays in `A(ω)`, but their drive vanishes). Read
///   each port's modal amplitude via [`waveguide_mode_reduce`]. The
///   self-term is subtracted on the diagonal.
///
/// Reciprocity (`Sᵀ = S`) holds for the complex-symmetric pencils this
/// solver assembles. A single propagating port at `a_inc = 1` and a
/// matched termination yields `|S₁₁| ≈ 0` (the wave is absorbed
/// completely); a straight section terminated in matched wave ports
/// yields `S₂₁ ≈ exp(−jβℓ)` (the phase advance through the section).
///
/// # Errors
///
/// Any [`DrivenError`] from assembly or the per-ω factorizations /
/// solves (the sweep stops at the first failure).
pub fn solve_wave_port_sweep<B: burn::tensor::backend::Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[WavePort],
    omegas: &[f64],
    device: &B::Device,
) -> Result<Vec<WavePortSweepPoint>, DrivenError> {
    if ports.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "wave-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    let edges = mesh.edges();
    let n_edges = edges.len();
    let n = ports.len();

    // --- Pre-compute the port-face modal flux f_m = S_p · e_m (real,
    // full-length) and validate. These are ω-independent.
    let fluxes: Vec<Vec<f64>> = ports
        .iter()
        .enumerate()
        .map(|(p_idx, port)| {
            if port.mode.len() != n_edges {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: format!(
                        "wave-port mode profile length {} must match edge count {}",
                        port.mode.len(),
                        n_edges
                    ),
                });
            }
            if port.a_inc == c64::new(0.0, 0.0) {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: "every wave port needs a non-zero a_inc to serve as an excitation"
                        .to_string(),
                });
            }
            Ok(assemble_modal_flux(mesh, &port.faces, &port.mode, &edges))
        })
        .collect::<Result<_, _>>()?;

    // --- Assemble the volume operator once (no lumped ports / surfaces).
    let zero_source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let no_ports: [LumpedPort<'_>; 0] = [];
    let no_surfaces: [SurfaceImpedanceBc<'_>; 0] = [];
    let op = DrivenOperator::assemble::<B>(
        mesh,
        materials,
        sigma_tet,
        bcs,
        &no_ports,
        &no_surfaces,
        &zero_source,
        device,
    )?;

    omegas
        .iter()
        .map(|&omega| {
            // β per port at this ω.
            let betas: Vec<c64> = ports.iter().map(|p| p.beta(omega)).collect();

            // Per-port full-length drive vectors: f_full_p = jβ_p · f_m
            // and modal-coupling vectors for the system rank-1 update.
            // Interior-filter f_m to align with op.n_interior.
            let n_int = op.n_interior();
            let fluxes_int: Vec<Vec<c64>> = fluxes
                .iter()
                .map(|f| {
                    let mut out = Vec::with_capacity(n_int);
                    for (full_idx, &val) in f.iter().enumerate() {
                        // The DrivenOperator does not expose its PEC mask
                        // directly; we use the same mask we built it
                        // with via bcs.
                        if bcs.pec_interior_mask[full_idx] {
                            out.push(c64::new(val, 0.0));
                        }
                    }
                    out
                })
                .collect();

            // Wave-port rank-1 system contribution and per-excitation
            // RHS drives are baked into a hand-built factor-once
            // operator. We do this via the existing factor_at path with
            // additional rank-1 modifications using
            // Sherman-Morrison-Woodbury — but the simpler route is to
            // build A_total(ω) = A_base(ω) + Σ_p jβ_p f_p f_pᵀ once per
            // ω, factor it, and solve N RHS.
            //
            // We rely on DrivenOperator's factor_at to build A_base(ω);
            // then we materialize the dense rank-N correction in a
            // separate sparse triplet list and form A_total. Since the
            // factor_at API returns a factored handle without exposing
            // A_int, we open-code the assembly here using the same
            // primitives.
            //
            // The cleanest path: use op.factor_at for A_base then apply
            // SMW to back-substitute. This avoids exposing internal
            // sparsity. With N ports and N excitations the SMW cost is
            // negligible.
            let factored = op.factor_at(omega)?;

            // Per-port system contribution: jβ_p · f_p f_pᵀ.
            // SMW: A_total = A_base + U Σ Vᵀ, with U = [u_1 ... u_n],
            // V = [v_1 ... v_n], Σ = diag(jβ_p). Here u_p = v_p = f_p
            // (real). Then for any RHS b: x = A_base⁻¹ b − A_base⁻¹ U
            // (Σ⁻¹ + Vᵀ A_base⁻¹ U)⁻¹ Vᵀ A_base⁻¹ b.

            // Precompute A_base⁻¹ U as N columns (interior).
            let mut ainv_u: Vec<Vec<c64>> = Vec::with_capacity(n);
            for col in fluxes_int.iter() {
                let mut x = vec![c64::new(0.0, 0.0); n_int];
                // Solve A_base x = col.
                crate::wave_port::factored_solve_into(&factored, col, &mut x)?;
                ainv_u.push(x);
            }
            // Capacitance matrix C = Σ⁻¹ + Vᵀ A_base⁻¹ U.
            // Here V = U = fluxes_int. We invert C (n×n dense).
            let mut cap = vec![c64::new(0.0, 0.0); n * n];
            for i in 0..n {
                for j in 0..n {
                    let mut acc = c64::new(0.0, 0.0);
                    for r in 0..n_int {
                        acc += fluxes_int[i][r] * ainv_u[j][r];
                    }
                    cap[i * n + j] = acc;
                }
                // Add Σ⁻¹ = 1/(jβ_i) on diagonal (skip if β_i = 0:
                // evanescent at exact cutoff — rank-1 term vanishes).
                let beta_i = betas[i];
                if beta_i.norm_sqr() > 0.0 {
                    let inv_jb = c64::new(0.0, -1.0) / beta_i;
                    cap[i * n + i] += inv_jb;
                }
            }
            // Invert cap (n × n).
            let cap_inv = invert_complex_dense(&cap, n).ok_or_else(|| {
                DrivenError::Solve(format!(
                    "wave-port SMW capacitance matrix singular at ω = {omega}"
                ))
            })?;

            // Per-excitation: drive port `j` at a_inc=ports[j].a_inc,
            // others at 0. The RHS is b_j(ω) = Σ_p 2jβ_p · a_inc_p · f_p,
            // restricted to p = j. So b_j = 2jβ_j · a_inc_j · f_j.
            //
            // Solve A_total x = b_j, then read each port's modal amp.
            let mut s = vec![c64::new(0.0, 0.0); n * n];
            let mut residual_rel = 0.0_f64;
            for j in 0..n {
                let drive_coeff = c64::new(0.0, 2.0) * betas[j] * ports[j].a_inc;
                // Build b = drive_coeff · f_j (interior).
                let b: Vec<c64> = fluxes_int[j].iter().map(|&x| x * drive_coeff).collect();

                // x = A_base⁻¹ b − (A_base⁻¹ U) C⁻¹ Uᵀ A_base⁻¹ b.
                let mut ainv_b = vec![c64::new(0.0, 0.0); n_int];
                crate::wave_port::factored_solve_into(&factored, &b, &mut ainv_b)?;
                // y = Uᵀ A_base⁻¹ b  (length n).
                let mut y = vec![c64::new(0.0, 0.0); n];
                for i in 0..n {
                    let mut acc = c64::new(0.0, 0.0);
                    for r in 0..n_int {
                        acc += fluxes_int[i][r] * ainv_b[r];
                    }
                    y[i] = acc;
                }
                // z = C⁻¹ y.
                let mut z = vec![c64::new(0.0, 0.0); n];
                for i in 0..n {
                    let mut acc = c64::new(0.0, 0.0);
                    for k in 0..n {
                        acc += cap_inv[i * n + k] * y[k];
                    }
                    z[i] = acc;
                }
                // x = ainv_b − Σ_i (A_base⁻¹ U)_i · z_i.
                let mut x = ainv_b.clone();
                for i in 0..n {
                    for r in 0..n_int {
                        x[r] -= ainv_u[i][r] * z[i];
                    }
                }
                // Residual check: ‖(A_base + Σ jβ f_pf_pᵀ) x − b‖ / ‖b‖.
                // We use the explicit residual for the corrected system.
                let mut ax = vec![c64::new(0.0, 0.0); n_int];
                crate::wave_port::factored_spmv(&factored, &x, &mut ax);
                for p in 0..n {
                    let beta_p = betas[p];
                    if beta_p.norm_sqr() == 0.0 {
                        continue;
                    }
                    let coeff = c64::new(0.0, 1.0) * beta_p;
                    let mut dot = c64::new(0.0, 0.0);
                    for r in 0..n_int {
                        dot += fluxes_int[p][r] * x[r];
                    }
                    let scaled = coeff * dot;
                    for r in 0..n_int {
                        ax[r] += fluxes_int[p][r] * scaled;
                    }
                }
                let (res_n2, b_n2) =
                    ax.iter()
                        .zip(b.iter())
                        .fold((0.0_f64, 0.0_f64), |(a, c), (ax_i, b_i)| {
                            let d = *ax_i - *b_i;
                            (
                                a + d.re * d.re + d.im * d.im,
                                c + b_i.re * b_i.re + b_i.im * b_i.im,
                            )
                        });
                if b_n2 > 0.0 {
                    residual_rel = residual_rel.max((res_n2 / b_n2).sqrt());
                }
                // Scatter to full edge vector.
                let mut e_edges = vec![c64::new(0.0, 0.0); n_edges];
                let mut interior_idx = 0;
                for (full_idx, &keep) in bcs.pec_interior_mask.iter().enumerate() {
                    if keep {
                        e_edges[full_idx] = x[interior_idx];
                        interior_idx += 1;
                    }
                }
                // Read each port's modal amplitude.
                for k in 0..n {
                    let a_k = waveguide_mode_reduce(mesh, &ports[k], &edges, &e_edges);
                    // S_kj: subtract incident self-term on diagonal.
                    let s_kj = if k == j {
                        (a_k - ports[j].a_inc) / ports[j].a_inc
                    } else {
                        a_k / ports[j].a_inc
                    };
                    s[k * n + j] = s_kj;
                }
            }
            Ok(WavePortSweepPoint {
                omega,
                residual_rel,
                s,
                beta: betas,
            })
        })
        .collect()
}

/// `factored.solve(b)` wrapper that puts the result into `out`. The
/// `FactoredDrivenOperator` exposes a `solve()` that builds its own RHS;
/// we need a back-substitution-only path here. Provided via a friend
/// helper on `crate::driven`.
pub(crate) fn factored_solve_into(
    factored: &FactoredDrivenOperator<'_>,
    b: &[c64],
    out: &mut [c64],
) -> Result<(), DrivenError> {
    factored.back_solve(b, out)
}

/// Compute `out = A_base · x` using the factored operator's cached
/// sparse matrix. Used for the residual check in [`solve_wave_port_sweep`].
pub(crate) fn factored_spmv(factored: &FactoredDrivenOperator<'_>, x: &[c64], out: &mut [c64]) {
    factored.spmv_a(x, out)
}

/// Dense Gauss-Jordan inversion of a row-major `n × n` complex matrix.
/// Returns `None` on an exactly singular pivot. For the port-count
/// matrices this serves (n = number of ports) a dense elimination is
/// the right tool — mirrors the `invert_complex` helper in
/// [`crate::extraction`].
fn invert_complex_dense(m: &[c64], n: usize) -> Option<Vec<c64>> {
    debug_assert_eq!(m.len(), n * n);
    let mut a = m.to_vec();
    let mut inv: Vec<c64> = (0..n * n)
        .map(|idx| {
            if idx % (n + 1) == 0 {
                c64::new(1.0, 0.0)
            } else {
                c64::new(0.0, 0.0)
            }
        })
        .collect();
    for col in 0..n {
        let mut piv = col;
        let mut piv_norm = a[col * n + col].norm();
        for r in (col + 1)..n {
            let v = a[r * n + col].norm();
            if v > piv_norm {
                piv = r;
                piv_norm = v;
            }
        }
        if piv_norm == 0.0 || piv_norm.is_nan() {
            return None;
        }
        if piv != col {
            for c in 0..n {
                a.swap(col * n + c, piv * n + c);
                inv.swap(col * n + c, piv * n + c);
            }
        }
        let d = a[col * n + col];
        for c in 0..n {
            a[col * n + c] /= d;
            inv[col * n + c] /= d;
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = a[r * n + col];
            if f.re == 0.0 && f.im == 0.0 {
                continue;
            }
            for c in 0..n {
                let av = a[col * n + c] * f;
                a[r * n + c] -= av;
                let iv = inv[col * n + c] * f;
                inv[r * n + c] -= iv;
            }
        }
    }
    Some(inv)
}

// =====================================================================
// Programmatic 3-D rectangular waveguide section fixture
// =====================================================================

/// Generate a tetrahedralized rectangular waveguide section
/// `[0,a] × [0,b] × [0,L]` with `(nx, ny, nz)` hex cells per side, each
/// hex split into 6 tets sharing the long body diagonal — the 3-D
/// extension of [`crate::cube_tet_mesh`]. The cross-section at any
/// `z = const` plane is exactly the 2-D mesh produced by
/// [`crate::rect_tri_mesh`] with the same `(nx, ny)`.
///
/// Returns the mesh plus three helper outputs:
/// - the port-1 face triangle list (`z = 0` plane),
/// - the port-2 face triangle list (`z = L` plane),
/// - the PEC sidewall triangle list (the four walls `x ∈ {0, a}` and
///   `y ∈ {0, b}`).
///
/// The port-face triangulation matches `rect_tri_mesh(nx, ny, a, b)`
/// exactly (same vertex pattern, same diagonal split), so the 2-D
/// modal eigenvector indexed in the 2-D port-mesh edge table maps
/// edge-for-edge into the 3-D mesh edge table via
/// [`map_mode_profile_to_full_mesh`].
pub fn extruded_rect_waveguide_mesh(
    nx: usize,
    ny: usize,
    nz: usize,
    a: f64,
    b: f64,
    length: f64,
) -> ExtrudedWaveguideMesh {
    assert!(
        nx >= 1 && ny >= 1 && nz >= 1,
        "extruded waveguide requires nx, ny, nz ≥ 1"
    );
    use std::collections::BTreeMap;
    let npx = nx + 1;
    let npy = ny + 1;
    let npz = nz + 1;
    let hx = a / nx as f64;
    let hy = b / ny as f64;
    let hz = length / nz as f64;

    let node_idx = |i: usize, j: usize, k: usize| -> u32 { (i + j * npx + k * npx * npy) as u32 };

    let mut nodes = Vec::with_capacity(npx * npy * npz);
    for k in 0..npz {
        for j in 0..npy {
            for i in 0..npx {
                nodes.push([i as f64 * hx, j as f64 * hy, k as f64 * hz]);
            }
        }
    }

    let mut tets = Vec::with_capacity(6 * nx * ny * nz);
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let c = [
                    node_idx(i, j, k),
                    node_idx(i + 1, j, k),
                    node_idx(i + 1, j + 1, k),
                    node_idx(i, j + 1, k),
                    node_idx(i, j, k + 1),
                    node_idx(i + 1, j, k + 1),
                    node_idx(i + 1, j + 1, k + 1),
                    node_idx(i, j + 1, k + 1),
                ];
                // 6-tet split sharing diagonal c[0]→c[6]. All right-handed.
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }

    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };

    // Port-1 triangle list (z = 0): use the same diagonal split as
    // rect_tri_mesh.
    let mut port1_faces: Vec<[u32; 3]> = Vec::with_capacity(2 * nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let c00 = node_idx(i, j, 0);
            let c10 = node_idx(i + 1, j, 0);
            let c11 = node_idx(i + 1, j + 1, 0);
            let c01 = node_idx(i, j + 1, 0);
            port1_faces.push([c00, c10, c11]);
            port1_faces.push([c00, c11, c01]);
        }
    }
    let mut port2_faces: Vec<[u32; 3]> = Vec::with_capacity(2 * nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let c00 = node_idx(i, j, nz);
            let c10 = node_idx(i + 1, j, nz);
            let c11 = node_idx(i + 1, j + 1, nz);
            let c01 = node_idx(i, j + 1, nz);
            port2_faces.push([c00, c10, c11]);
            port2_faces.push([c00, c11, c01]);
        }
    }

    // PEC sidewalls. We collect all boundary triangles on the four
    // walls x = 0, x = a, y = 0, y = b. The wall triangulations on the
    // tetrahedralized hexes follow the same 6-tet split pattern; the
    // cleanest enumeration is to walk all tet faces and select those
    // lying flat on a sidewall.
    let tol = 1e-9 * a.max(b).max(length).max(1.0);
    let on_wall = |p: [f64; 3]| -> bool {
        p[0].abs() < tol || (p[0] - a).abs() < tol || p[1].abs() < tol || (p[1] - b).abs() < tol
    };
    let on_z_plane = |p: [f64; 3], z: f64| -> bool { (p[2] - z).abs() < tol };
    let mut sidewall_faces: Vec<[u32; 3]> = Vec::new();
    for tet in &mesh.tets {
        let coords: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
        for lf in &crate::mesh::TET_LOCAL_FACES {
            let tri_pts = [coords[lf[0]], coords[lf[1]], coords[lf[2]]];
            // Sidewall if all 3 vertices on same wall and NOT on port plane.
            if tri_pts.iter().all(|&p| on_wall(p))
                && !tri_pts.iter().all(|&p| on_z_plane(p, 0.0))
                && !tri_pts.iter().all(|&p| on_z_plane(p, length))
            {
                // Check that all three points lie on the same single wall
                // (not just on any wall — a tet-face along a corner edge
                // could touch two walls).
                let same_x0 = tri_pts.iter().all(|p| p[0].abs() < tol);
                let same_xa = tri_pts.iter().all(|p| (p[0] - a).abs() < tol);
                let same_y0 = tri_pts.iter().all(|p| p[1].abs() < tol);
                let same_yb = tri_pts.iter().all(|p| (p[1] - b).abs() < tol);
                if same_x0 || same_xa || same_y0 || same_yb {
                    let tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
                    sidewall_faces.push(tri);
                }
            }
        }
    }

    ExtrudedWaveguideMesh {
        mesh,
        port1_faces,
        port2_faces,
        sidewall_faces,
        a,
        b,
        length,
    }
}

/// Output of [`extruded_rect_waveguide_mesh`]: the volume mesh plus the
/// boundary face lists needed to build the wave-port BC + PEC sidewall
/// elimination.
#[derive(Debug, Clone)]
pub struct ExtrudedWaveguideMesh {
    pub mesh: TetMesh,
    /// Port-1 face triangles on `z = 0`.
    pub port1_faces: Vec<[u32; 3]>,
    /// Port-2 face triangles on `z = length`.
    pub port2_faces: Vec<[u32; 3]>,
    /// Sidewall PEC face triangles on `x ∈ {0, a}` and `y ∈ {0, b}`.
    pub sidewall_faces: Vec<[u32; 3]>,
    pub a: f64,
    pub b: f64,
    pub length: f64,
}

impl ExtrudedWaveguideMesh {
    /// PEC interior-edge mask: edges are kept (interior) unless they
    /// lie on a sidewall — port-face edges are kept (the wave port
    /// substitutes for the PEC there). This is the BC mask the
    /// wave-port driven solve expects.
    pub fn pec_interior_mask(&self) -> Vec<bool> {
        let edges = self.mesh.edges();
        crate::mesh::pec_interior_mask_from_triangles(&edges, &[self.sidewall_faces.as_slice()])
    }
}

// =====================================================================
// Programmatic 3-D height-step waveguide fixture (issue #248)
// =====================================================================

/// Generate a tetrahedralized **height-step** rectangular waveguide:
/// section A `[0,a] × [0,b1] × [0,L1]` joined at `z = L1` to section B
/// `[0,a] × [0,b2] × [L1, L1 + L2]`. The two sections share the bottom
/// wall `y = 0` and the side walls `x ∈ {0, a}`; section B has
/// `b2 < b1`, so at `z = L1` the annular strip `y ∈ [b2, b1]` becomes a
/// new PEC backwall (the "step face"). Section A's top wall `y = b1`
/// and section B's top wall `y = b2` are PEC. Wave ports live on the
/// end faces `z = 0` (cross-section `a × b1`) and `z = L1 + L2`
/// (cross-section `a × b2`).
///
/// To keep the two sub-meshes node-conforming at the interface
/// `z = L1`, both halves share the **same horizontal discretization**:
/// `nx` cells across `[0, a]` and a single `hy` chosen so that `b1` and
/// `b2` are both integer multiples of `hy`. The caller passes
/// `(nx, ny1, ny2)` with the implicit invariant `b1 * ny2 == b2 * ny1`
/// (the same `hy = b1/ny1 = b2/ny2`); the function asserts this. The
/// `(nz1, nz2)` cell counts control the z resolution of each half
/// independently.
///
/// The cross-section of port 1 at `z = 0` is the 2-D mesh produced by
/// `rect_tri_mesh(nx, ny1, a, b1)`; the cross-section of port 2 at
/// `z = L1 + L2` is the 2-D mesh produced by
/// `rect_tri_mesh(nx, ny2, a, b2)`. This matches the assumption in the
/// wave-port BC machinery: per-port modal solves run independently on
/// each port's own 2-D mesh (different `b` → different modal basis),
/// and [`map_mode_profile_to_full_mesh`] stitches each profile into the
/// 3-D edge table by node-tag matching against this fixture's nodes.
///
/// # Panics
///
/// - if `nx, ny1, ny2, nz1, nz2 < 1`,
/// - if `b1 * ny2 != b2 * ny1` (the implicit `hy` invariant).
#[allow(clippy::too_many_arguments)]
pub fn extruded_height_step_waveguide_mesh(
    nx: usize,
    ny1: usize,
    ny2: usize,
    nz1: usize,
    nz2: usize,
    a: f64,
    b1: f64,
    b2: f64,
    l1: f64,
    l2: f64,
) -> ExtrudedHeightStepMesh {
    assert!(
        nx >= 1 && ny1 >= 1 && ny2 >= 1 && nz1 >= 1 && nz2 >= 1,
        "height-step waveguide requires nx, ny1, ny2, nz1, nz2 ≥ 1"
    );
    // Shared hy invariant.
    let hy_a = b1 / ny1 as f64;
    let hy_b = b2 / ny2 as f64;
    assert!(
        (hy_a - hy_b).abs() < 1e-12 * hy_a.max(hy_b).max(1.0),
        "height-step waveguide needs b1/ny1 == b2/ny2 (b1={b1}, ny1={ny1}, b2={b2}, ny2={ny2})"
    );
    assert!(
        ny2 <= ny1,
        "height-step waveguide expects the smaller section B (ny2 ≤ ny1); got ny1={ny1}, ny2={ny2}"
    );

    use std::collections::BTreeMap;
    let npx = nx + 1;
    let npy_a = ny1 + 1;
    let npy_b = ny2 + 1;
    let npz_a = nz1 + 1;
    let npz_b = nz2 + 1;
    let hx = a / nx as f64;
    let hy = hy_a;
    let hz_a = l1 / nz1 as f64;
    let hz_b = l2 / nz2 as f64;

    // --- Section A nodes: full lattice [0..npx] × [0..npy_a] × [0..npz_a]
    // indexed lex-order (i, j, k) → i + j*npx + k*npx*npy_a.
    let n_a = npx * npy_a * npz_a;
    let node_a = |i: usize, j: usize, k: usize| -> u32 { (i + j * npx + k * npx * npy_a) as u32 };

    // --- Section B nodes live in a separate index range. Their k=0
    // slice (z = L1) must reuse section A's k=nz1 nodes for j ∈ [0, ny2]
    // — same (x, y) coordinates. We do this by *not allocating* new
    // nodes for B's k=0 layer; we'll point B's k=0 indices at A's
    // k=nz1, j∈[0,ny2] nodes through a translation table.
    let b_layer_size = npx * npy_b;
    let n_b_new = b_layer_size * nz2; // layers k = 1..=nz2 are new
    let node_b = |i: usize, j: usize, k: usize| -> u32 {
        // k = 0: alias into section A at k = nz1, j ∈ [0, ny2].
        if k == 0 {
            node_a(i, j, nz1)
        } else {
            (n_a + i + j * npx + (k - 1) * b_layer_size) as u32
        }
    };

    let mut nodes = Vec::with_capacity(n_a + n_b_new);
    for k in 0..npz_a {
        for j in 0..npy_a {
            for i in 0..npx {
                nodes.push([i as f64 * hx, j as f64 * hy, k as f64 * hz_a]);
            }
        }
    }
    for k in 1..npz_b {
        for j in 0..npy_b {
            for i in 0..npx {
                nodes.push([i as f64 * hx, j as f64 * hy, l1 + k as f64 * hz_b]);
            }
        }
    }
    debug_assert_eq!(nodes.len(), n_a + n_b_new);

    // --- Section A tets (same 6-tet split as extruded_rect_waveguide_mesh).
    let mut tets = Vec::with_capacity(6 * nx * ny1 * nz1 + 6 * nx * ny2 * nz2);
    for k in 0..nz1 {
        for j in 0..ny1 {
            for i in 0..nx {
                let c = [
                    node_a(i, j, k),
                    node_a(i + 1, j, k),
                    node_a(i + 1, j + 1, k),
                    node_a(i, j + 1, k),
                    node_a(i, j, k + 1),
                    node_a(i + 1, j, k + 1),
                    node_a(i + 1, j + 1, k + 1),
                    node_a(i, j + 1, k + 1),
                ];
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }
    // --- Section B tets.
    for k in 0..nz2 {
        for j in 0..ny2 {
            for i in 0..nx {
                let c = [
                    node_b(i, j, k),
                    node_b(i + 1, j, k),
                    node_b(i + 1, j + 1, k),
                    node_b(i, j + 1, k),
                    node_b(i, j, k + 1),
                    node_b(i + 1, j, k + 1),
                    node_b(i + 1, j + 1, k + 1),
                    node_b(i, j + 1, k + 1),
                ];
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }

    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };

    // --- Port 1 face triangles (z = 0, full section A cross-section).
    let mut port1_faces: Vec<[u32; 3]> = Vec::with_capacity(2 * nx * ny1);
    for j in 0..ny1 {
        for i in 0..nx {
            let c00 = node_a(i, j, 0);
            let c10 = node_a(i + 1, j, 0);
            let c11 = node_a(i + 1, j + 1, 0);
            let c01 = node_a(i, j + 1, 0);
            port1_faces.push([c00, c10, c11]);
            port1_faces.push([c00, c11, c01]);
        }
    }
    // --- Port 2 face triangles (z = L1 + L2, section B cross-section).
    let mut port2_faces: Vec<[u32; 3]> = Vec::with_capacity(2 * nx * ny2);
    for j in 0..ny2 {
        for i in 0..nx {
            let c00 = node_b(i, j, nz2);
            let c10 = node_b(i + 1, j, nz2);
            let c11 = node_b(i + 1, j + 1, nz2);
            let c01 = node_b(i, j + 1, nz2);
            port2_faces.push([c00, c10, c11]);
            port2_faces.push([c00, c11, c01]);
        }
    }

    // --- Sidewall PEC faces. Walk every tet face; a face is a PEC
    // sidewall iff all three vertices share one of:
    //   (1) `x = 0` plane,
    //   (2) `x = a` plane,
    //   (3) `y = 0` plane (shared floor),
    //   (4) section A top: `y = b1` AND `z ∈ [0, L1]`,
    //   (5) section B top: `y = b2` AND `z ∈ [L1, L1 + L2]`,
    //   (6) **the step backwall**: `z = L1` AND `y ∈ [b2, b1]` (i.e.
    //       the annular face that closes the volume where section B
    //       narrowed).
    // We exclude the two port planes (handled separately as wave ports).
    let tol_xyz = 1e-9 * a.max(b1.max(b2)).max(l1.max(l2)).max(1.0);
    let mut sidewall_faces: Vec<[u32; 3]> = Vec::new();
    for tet in &mesh.tets {
        let coords: [[f64; 3]; 4] = std::array::from_fn(|v| mesh.nodes[tet[v] as usize]);
        for lf in &crate::mesh::TET_LOCAL_FACES {
            let tri_pts = [coords[lf[0]], coords[lf[1]], coords[lf[2]]];
            // Exclude port planes.
            let on_port1 = tri_pts.iter().all(|p| p[2].abs() < tol_xyz);
            let on_port2 = tri_pts.iter().all(|p| (p[2] - (l1 + l2)).abs() < tol_xyz);
            if on_port1 || on_port2 {
                continue;
            }
            let same_x0 = tri_pts.iter().all(|p| p[0].abs() < tol_xyz);
            let same_xa = tri_pts.iter().all(|p| (p[0] - a).abs() < tol_xyz);
            let same_y0 = tri_pts.iter().all(|p| p[1].abs() < tol_xyz);
            // Section A top: y = b1 AND z ∈ [0, L1].
            let on_a_top = tri_pts
                .iter()
                .all(|p| (p[1] - b1).abs() < tol_xyz && p[2] <= l1 + tol_xyz);
            // Section B top: y = b2 AND z ∈ [L1, L1 + L2].
            let on_b_top = tri_pts
                .iter()
                .all(|p| (p[1] - b2).abs() < tol_xyz && p[2] >= l1 - tol_xyz);
            // Step backwall: z = L1 AND y ∈ [b2, b1].
            let on_step = tri_pts.iter().all(|p| {
                (p[2] - l1).abs() < tol_xyz && p[1] >= b2 - tol_xyz && p[1] <= b1 + tol_xyz
            });
            if same_x0 || same_xa || same_y0 || on_a_top || on_b_top || on_step {
                let tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
                sidewall_faces.push(tri);
            }
        }
    }

    ExtrudedHeightStepMesh {
        mesh,
        port1_faces,
        port2_faces,
        sidewall_faces,
        a,
        b1,
        b2,
        l1,
        l2,
    }
}

/// Output of [`extruded_height_step_waveguide_mesh`]: the volume mesh
/// plus the boundary face lists needed to build the two wave-port BCs
/// (different cross-sections) and the PEC elimination over the combined
/// sidewall + step-backwall surface.
#[derive(Debug, Clone)]
pub struct ExtrudedHeightStepMesh {
    pub mesh: TetMesh,
    /// Port-1 face triangles on `z = 0`, section A cross-section
    /// `a × b1`.
    pub port1_faces: Vec<[u32; 3]>,
    /// Port-2 face triangles on `z = L1 + L2`, section B cross-section
    /// `a × b2`.
    pub port2_faces: Vec<[u32; 3]>,
    /// PEC face triangles on the four side walls + section A top
    /// (`y = b1`, `z ∈ [0, L1]`) + section B top (`y = b2`,
    /// `z ∈ [L1, L1 + L2]`) + step backwall (`z = L1`, `y ∈ [b2, b1]`).
    pub sidewall_faces: Vec<[u32; 3]>,
    pub a: f64,
    pub b1: f64,
    pub b2: f64,
    pub l1: f64,
    pub l2: f64,
}

impl ExtrudedHeightStepMesh {
    /// PEC interior-edge mask: edges are kept (interior) unless they
    /// lie on a sidewall / step backwall — port-face edges are kept
    /// (the wave port substitutes for the PEC there). Same contract as
    /// [`ExtrudedWaveguideMesh::pec_interior_mask`].
    pub fn pec_interior_mask(&self) -> Vec<bool> {
        let edges = self.mesh.edges();
        crate::mesh::pec_interior_mask_from_triangles(&edges, &[self.sidewall_faces.as_slice()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveguide_modes::{rect_tri_mesh, solve_rect_waveguide_modes, TriMesh};

    #[test]
    fn extruded_waveguide_mesh_shapes_are_consistent() {
        let nx = 4;
        let ny = 2;
        let nz = 3;
        let (a, b, l) = (2.0, 1.0, 3.0);
        let g = extruded_rect_waveguide_mesh(nx, ny, nz, a, b, l);
        assert_eq!(g.mesh.n_nodes(), (nx + 1) * (ny + 1) * (nz + 1));
        assert_eq!(g.mesh.n_tets(), 6 * nx * ny * nz);
        // Two triangles per quad on each port face.
        assert_eq!(g.port1_faces.len(), 2 * nx * ny);
        assert_eq!(g.port2_faces.len(), 2 * nx * ny);
        // Sidewalls = 4 walls, each rect_tri-meshed by (perp × length) quads.
        // (Each wall yields 2 triangles per quad in the structured split.)
        // x = 0 and x = a walls: ny * nz quads each.
        // y = 0 and y = b walls: nx * nz quads each.
        let expected_sidewall_tris = 2 * (2 * (ny * nz) + 2 * (nx * nz));
        assert_eq!(g.sidewall_faces.len(), expected_sidewall_tris);
    }

    #[test]
    fn map_mode_profile_round_trips() {
        // 2-D port mesh.
        let (nx, ny) = (2, 1);
        let (a, b) = (2.0, 1.0);
        let port = rect_tri_mesh(nx, ny, a, b);
        let port_edges_2d = port.edges();
        // Synthetic profile: edge index as value.
        let mode_2d: Vec<f64> = (0..port_edges_2d.len()).map(|i| (i + 1) as f64).collect();

        // 3-D mesh with same cross-section.
        let g = extruded_rect_waveguide_mesh(nx, ny, 2, a, b, 4.0);
        let edges_3d = g.mesh.edges();
        let mapped = map_mode_profile_to_full_mesh(&port_edges_2d, &mode_2d, &edges_3d);

        // Every nonzero mapped slot must hit a port-face edge (z=0
        // plane) since the 2-D port mesh is the z=0 face triangulation.
        let z_tol = 1e-12;
        for (i, &v) in mapped.iter().enumerate() {
            if v != 0.0 {
                let e = edges_3d[i];
                let p0 = g.mesh.nodes[e[0] as usize];
                let p1 = g.mesh.nodes[e[1] as usize];
                assert!(
                    p0[2].abs() < z_tol && p1[2].abs() < z_tol,
                    "mapped nonzero on non-port edge {:?}",
                    e
                );
            }
        }
        // Count: should match nonzero entries of mode_2d.
        let n_nonzero_2d = mode_2d.iter().filter(|&&v| v != 0.0).count();
        let n_nonzero_3d = mapped.iter().filter(|&&v| v != 0.0).count();
        assert_eq!(n_nonzero_2d, n_nonzero_3d);
    }

    #[test]
    fn waveguide_mode_profile_orthonormal_in_2d() {
        // The 2-D modal solver normalizes eᵀ M e = 1 over interior
        // edges; a quick sanity check that the eigenvector is
        // non-trivial.
        let (a, b) = (2.0, 1.0);
        let mesh = rect_tri_mesh(8, 4, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 1).expect("2-D modal solve");
        assert_eq!(modes.len(), 1);
        let m = &modes[0];
        let nonzero = m.e_edges.iter().filter(|&&v| v != 0.0).count();
        assert!(nonzero > 0, "modal profile is all-zero");
        // Cutoff matches TE₁₀ to a few %.
        let pi = std::f64::consts::PI;
        let kc = pi / a;
        let rel = (m.k_c - kc).abs() / kc;
        assert!(rel < 0.05, "TE10 k_c err {rel}");
    }

    /// Trivial dimension test for the helper TriMesh that mirrors what
    /// we need at the 2-D / 3-D boundary.
    #[test]
    fn trimesh_2d_smoke() {
        let m: TriMesh = rect_tri_mesh(2, 2, 1.0, 1.0);
        assert_eq!(m.n_tris(), 8);
    }

    #[test]
    fn height_step_mesh_node_and_tet_counts() {
        // a × b1 × L1 joined to a × b2 × L2, with shared hy.
        // b1 = 1.0, ny1 = 4 → hy = 0.25; b2 = 0.5, ny2 = 2 → hy = 0.25.
        let (nx, ny1, ny2, nz1, nz2) = (4, 4, 2, 3, 3);
        let (a, b1, b2, l1, l2) = (2.0, 1.0, 0.5, 1.0, 1.0);
        let g = extruded_height_step_waveguide_mesh(nx, ny1, ny2, nz1, nz2, a, b1, b2, l1, l2);
        // Section A: (nx+1)(ny1+1)(nz1+1) nodes; section B contributes
        // (nx+1)(ny2+1)(nz2) NEW nodes (k=0 layer is aliased).
        let expected_nodes = (nx + 1) * (ny1 + 1) * (nz1 + 1) + (nx + 1) * (ny2 + 1) * nz2;
        assert_eq!(g.mesh.n_nodes(), expected_nodes);
        // Tets: 6 per hex × hex count per section.
        let expected_tets = 6 * (nx * ny1 * nz1 + nx * ny2 * nz2);
        assert_eq!(g.mesh.n_tets(), expected_tets);
        assert_eq!(g.port1_faces.len(), 2 * nx * ny1);
        assert_eq!(g.port2_faces.len(), 2 * nx * ny2);
    }

    #[test]
    fn height_step_mesh_interface_is_node_conforming() {
        // The shared interface at z = L1 should have section A's
        // bottom-portion nodes (j ∈ [0, ny2]) re-used as section B's
        // k = 0 nodes (same node index, same coords). We verify by
        // checking that any face at z = L1 with y ∈ [0, b2] is referenced
        // by both section A and section B tets and is NOT a sidewall.
        let (nx, ny1, ny2, nz1, nz2) = (4, 4, 2, 2, 2);
        let (a, b1, b2, l1, l2) = (2.0, 1.0, 0.5, 0.8, 0.6);
        let g = extruded_height_step_waveguide_mesh(nx, ny1, ny2, nz1, nz2, a, b1, b2, l1, l2);
        let tol = 1e-9;
        // The step backwall (z = L1, y ∈ [b2, b1]) should be PEC.
        // Count: it has nx cells across x and (ny1 - ny2) cells across
        // y → 2 triangles per quad.
        let expected_step_tris = 2 * nx * (ny1 - ny2);
        let mut step_count = 0;
        for tri in &g.sidewall_faces {
            let pts = [
                g.mesh.nodes[tri[0] as usize],
                g.mesh.nodes[tri[1] as usize],
                g.mesh.nodes[tri[2] as usize],
            ];
            let on_step = pts
                .iter()
                .all(|p| (p[2] - l1).abs() < tol && p[1] >= b2 - tol && p[1] <= b1 + tol);
            if on_step {
                step_count += 1;
            }
        }
        assert_eq!(
            step_count, expected_step_tris,
            "step backwall: expected {expected_step_tris} triangles, got {step_count}"
        );
        // No sidewall face on the open interface z = L1, y ∈ [0, b2].
        for tri in &g.sidewall_faces {
            let pts = [
                g.mesh.nodes[tri[0] as usize],
                g.mesh.nodes[tri[1] as usize],
                g.mesh.nodes[tri[2] as usize],
            ];
            let on_open = pts
                .iter()
                .all(|p| (p[2] - l1).abs() < tol && p[1] <= b2 - tol);
            assert!(
                !on_open,
                "open interface face wrongly tagged as PEC: {:?}",
                pts
            );
        }
    }
}
