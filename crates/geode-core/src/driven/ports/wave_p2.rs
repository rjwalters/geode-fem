//! **p=2 geometric (homogeneous) wave ports** (issue #884, Epic #836
//! Phase 3a): the port-face mode solve, the modal flux and the
//! order-generic wave / mixed / spec sweeps on an
//! [`HcurlSpace`].
//!
//! # The face pencil is built in the 3-D trace layout
//!
//! The tangential trace of the 3-D second-order Nédélec space on a planar
//! port face is the 8-DOF second-order Nédélec triangle
//! ([`crate::assembly::surface_p2`]). The face pencil is assembled with
//! every triangle's vertices in **ascending order**, the convention of the
//! 3-D space, and with the triangle element
//! [`crate::analytic::waveguide::tri_nedelec2_local`] (its local layout
//! `[W₀, Q₀, W₁, Q₁, W₂, Q₂, λ₂W₀, λ₀W₂]` on vertices `0 < 1 < 2` is the
//! trace layout `[W_ab, Q_ab, W_ac, Q_ac, W_bc, Q_bc, φ0, φ1]`). The local
//! node numbering of [`PortFaceProjection`] is monotone in the 3-D node
//! tags, so ascending local order is ascending global order, and:
//!
//! - a face edge's `(W, Q)` pair is the 3-D edge's `(W, Q)` pair
//!   ([`HcurlSpace::edge_dofs`], `W` oriented low→high in both), and
//! - a face triangle's bubble pair is the 3-D face's `(φ0, φ1)` pair
//!   ([`HcurlSpace::face_dofs`]).
//!
//! The lift onto the 3-D DOF vector is therefore a pure scatter: unit sign
//! and no `2×2` face mixing ([`HcurlSpace::face_dof_transform`] is never
//! needed). The in-plane projection is an isometry, so the 2-D mass of the
//! face pencil **is** the 3-D tangential surface mass `S_p` of the p=2
//! trace kernel, and the lifted modes keep `e_iᵀ S_p e_j = δ_ij`.
//! `tests/wave_port_p2.rs` pins this.
//!
//! # Null space and mode count
//!
//! With the rim's `(W, Q)` eliminated, the face pencil's kernel is the
//! gradient image of the P2-Lagrange scalars that vanish on the rim (one per
//! interior node and one per interior edge) plus one harmonic field per
//! hole. The shift estimator is told that exact dimension, and
//! [`PortFaceProjection::solve_modes_p2`] rejects a request for more modes
//! than `2·E_int + 2·T − (N_int + E_int) − holes` up front.
//!
//! # Gauge
//!
//! The modes carry the reference-integral sign convention of the p=1 modes
//! (issue #300): the sign makes `∫ e_h · F dA` positive for the first
//! reference field `F` that overlaps the mode. The projection is a
//! quadrature over the face, so it is a continuous functional of the mode
//! and stable under refinement. The reference list starts with the p=1
//! list, in order, and continues with the TE_mn transverse shapes, so modes
//! beyond the p=1 list (TE₁₁, TE₂₁, …) are gauged too. A reference counts
//! only above 1 % of the Cauchy–Schwarz ceiling, so discretization noise in
//! a continuously orthogonal reference never sets the sign. A mode that no
//! reference spans is [`crate::eigen::dense::EigenError::UngaugableMode`],
//! as at p=1.
//!
//! # Sweeps
//!
//! [`solve_wave_port_sweep_on_space`], [`solve_mixed_port_sweep_on_space`]
//! and the spec sweeps [`solve_wave_port_spec_sweep_on_space`] /
//! [`solve_mixed_port_spec_sweep_on_space`]:
//!
//! - **p=1** calls the existing p=1 entry point verbatim, so the result is
//!   bit-identical to it.
//! - **p=2** builds the base operator with
//!   [`DrivenOperator::assemble_with_space`] (lumped ports and impedance
//!   walls through the #857 trace kernel) and the modal channels with the
//!   p=2 flux `f = S_p e`. The per-ω rank-`N` SMW post-step is the mixed
//!   sweep's, unchanged. Filled ports (#777, [`super::PortMedium`]) are
//!   order-agnostic, because the medium only changes the scalars `β` and
//!   `y`.
//!
//! **Hybrid ports** (inhomogeneous cross-sections) at p=2 return
//! [`DrivenError::UnsupportedAtOrder`]. They need a matched Nédélec-2 +
//! P2-Lagrange `E_z` face pencil, which is Epic #836 Phase 3b.

use std::collections::HashMap;

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use super::hybrid::{MixedPortSpecSweep, WavePortSpec, WavePortSpecSweep};
use super::lumped::LumpedPort;
use super::mixed::{MixedPortSweepPoint, ModalChannel, mixed_sweep_points, modal_channels_with};
use super::wave::{
    PortMedium, PortMode, WavePort, WavePortSweepPoint, assemble_modal_flux,
    solve_wave_port_sweep_with_mode,
};
use super::wave_face::{PortFaceError, PortFaceProjection};
use crate::analytic::waveguide::{
    TRI_QUAD_DEG4, estimate_modal_shift, metallic_checked_modes, tri_nedelec2_local,
};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::assembly::surface_p2::{TRI_NEDELEC2_TRACE_DOFS, assemble_p2_surface_mass_triplets};
use crate::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, DrivenSource,
    SolverMode, SurfaceImpedanceBc,
};
use crate::eigen::dense::EigenError;
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

const ND: usize = TRI_NEDELEC2_TRACE_DOFS;

/// One p=2 cross-section mode of a [`PortFaceProjection`]
/// ([`PortFaceProjection::solve_modes_p2`]).
#[derive(Debug, Clone)]
pub struct PortFaceModeP2 {
    /// Cutoff wavenumber `k_c` (geometric, empty guide).
    pub k_c: f64,
    /// The eigenvalue `λ = k_c²` of the face pencil.
    pub lambda: f64,
    /// The mode over the face's p=2 DOF layout
    /// ([`PortFaceProjection::n_dofs_p2`]): edge `e` of
    /// [`PortFaceProjection::edges`] owns `2e` (`W`) and `2e + 1` (`Q`),
    /// triangle `t` owns `2E + 2t` (`φ0`) and `2E + 2t + 1` (`φ1`), with
    /// `E` the face edge count. Rim `(W, Q)` entries are exact zeros. The
    /// set is orthonormal in the face mass (= the 3-D `S_p`).
    pub dofs: Vec<f64>,
}

/// The p=2 DOF layout of a projected port face (module docs).
struct FaceLayoutP2 {
    /// Per triangle: its local vertices in ascending order and its 8 face
    /// DOFs in the trace layout.
    tris: Vec<([u32; 3], [usize; ND])>,
    /// Kept (`true`) / rim-eliminated (`false`) per face DOF.
    keep: Vec<bool>,
}

impl FaceLayoutP2 {
    fn n_dofs(&self) -> usize {
        self.keep.len()
    }
}

impl PortFaceProjection {
    /// Number of p=2 DOFs of the face: `2·E + 2·T` (two per face edge, two
    /// per face triangle), the length of [`PortFaceModeP2::dofs`].
    pub fn n_dofs_p2(&self) -> usize {
        2 * self.edges.len() + 2 * self.tri_mesh.tris.len()
    }

    fn layout_p2(&self) -> FaceLayoutP2 {
        let n_edges = self.edges.len();
        let lookup: HashMap<(u32, u32), usize> = self
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let tris = self
            .tri_mesh
            .tris
            .iter()
            .enumerate()
            .map(|(t, tri)| {
                let mut s = *tri;
                s.sort_unstable();
                let mut d = [0usize; ND];
                for (k, (a, b)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
                    let e = lookup[&(s[a], s[b])];
                    d[2 * k] = 2 * e;
                    d[2 * k + 1] = 2 * e + 1;
                }
                d[6] = 2 * n_edges + 2 * t;
                d[7] = 2 * n_edges + 2 * t + 1;
                (s, d)
            })
            .collect();
        let mut keep = Vec::with_capacity(self.n_dofs_p2());
        for &interior in &self.interior_edge_mask {
            keep.push(interior);
            keep.push(interior);
        }
        keep.resize(self.n_dofs_p2(), true);
        FaceLayoutP2 { tris, keep }
    }

    /// The exact null dimension of the p=2 face pencil: the rim-vanishing
    /// P2-Lagrange gradient image (interior nodes + interior edges) plus one
    /// harmonic field per hole.
    fn null_dim_p2(&self) -> usize {
        self.n_interior_nodes() + self.n_interior_edges() + self.n_holes()
    }

    /// Solve the `n_modes` lowest-cutoff cross-section modes of the face with
    /// the **second-order** Nédélec face pencil, in the 3-D trace layout
    /// (module docs). This is the p=2 counterpart of
    /// [`Self::solve_modes`]. It is **TE only**, as at p=1: the TM guard
    /// ([`Self::tm_cutoff_estimate`]) still applies.
    ///
    /// The pass is residual-checked (#798). A short list is never
    /// returned: an undercount doubles the Lanczos budget up to the pencil
    /// dimension and then fails.
    ///
    /// # Errors
    ///
    /// [`PortFaceError::TooFewModes`] if `n_modes` exceeds the face's
    /// physical-mode count `2·E_int + 2·T − (N_int + E_int) − holes`, or the
    /// solve resolves fewer; [`PortFaceError::Modal`] if the eigensolve
    /// fails or a mode cannot be gauged.
    pub fn solve_modes_p2(&self, n_modes: usize) -> Result<Vec<PortFaceModeP2>, PortFaceError> {
        let layout = self.layout_p2();
        let n_keep = layout.keep.iter().filter(|&&k| k).count();
        let null_dim = self.null_dim_p2();
        let available = n_keep.saturating_sub(null_dim);
        if n_modes > available {
            return Err(PortFaceError::TooFewModes {
                requested: n_modes,
                found: available,
            });
        }
        let mut renumber = vec![usize::MAX; layout.n_dofs()];
        let mut interior_to_full = Vec::with_capacity(n_keep);
        for (i, &k) in layout.keep.iter().enumerate() {
            if k {
                renumber[i] = interior_to_full.len();
                interior_to_full.push(i);
            }
        }
        let dim = interior_to_full.len();
        let cap = 64 * layout.tris.len();
        let mut k_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
        let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
        for (s, d) in &layout.tris {
            let coords = s.map(|n| self.tri_mesh.nodes[n as usize]);
            // `tri_nedelec2_local` integrates with |area| and its gradients
            // carry the signed determinant, so it is valid for either winding
            // of the ascending triple.
            let (k_loc, m_loc, _signed) = tri_nedelec2_local(&coords);
            for i in 0..ND {
                let ri = renumber[d[i]];
                if ri == usize::MAX {
                    continue;
                }
                for j in 0..ND {
                    let rj = renumber[d[j]];
                    if rj == usize::MAX {
                        continue;
                    }
                    k_trips.push(Triplet::new(ri, rj, k_loc[i][j]));
                    m_trips.push(Triplet::new(ri, rj, m_loc[i][j]));
                }
            }
        }
        let sparse = |tr: &[Triplet<usize, usize, f64>]| {
            SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, tr).map_err(|e| {
                PortFaceError::Modal(EigenError::FaerGevd(format!(
                    "p=2 port-face pencil assembly: {e:?}"
                )))
            })
        };
        let k = sparse(&k_trips)?;
        let m = sparse(&m_trips)?;

        let (sigma, _first) = estimate_modal_shift(k.as_ref(), m.as_ref(), n_modes, null_dim)?;
        let threshold = 0.1 * sigma;
        let mut n_request = (n_modes + 8).min(dim);
        let pairs = loop {
            let pass = metallic_checked_modes(
                k.as_ref(),
                m.as_ref(),
                sigma,
                threshold,
                n_request,
                n_modes,
            )?;
            if pass.physical.len() == n_modes && !pass.unresolved_below {
                break pass.physical;
            }
            if n_request >= dim {
                return Err(PortFaceError::TooFewModes {
                    requested: n_modes,
                    found: pass.physical.len(),
                });
            }
            n_request = (n_request * 2).min(dim);
        };
        pairs
            .into_iter()
            .enumerate()
            .map(|(idx, pair)| {
                let mut dofs = vec![0.0_f64; layout.n_dofs()];
                for (i, &full) in interior_to_full.iter().enumerate() {
                    dofs[full] = pair.vector[i];
                }
                self.gauge_p2(&layout, &mut dofs, idx)?;
                let lambda = pair.lambda;
                Ok(PortFaceModeP2 {
                    k_c: lambda.max(0.0).sqrt(),
                    lambda,
                    dofs,
                })
            })
            .collect()
    }

    /// The reference-integral gauge of issue #300 at p=2 (module docs): flip
    /// the mode so `∫ e_h · F dA` is positive for the first reference field
    /// `F` it overlaps by more than [`P2_GAUGE_FLOOR`] of the Cauchy–Schwarz
    /// ceiling `‖e‖·‖F‖`.
    ///
    /// The references start with the six fields of the p=1 gauge, in the same
    /// order, and continue with the TE_mn transverse shapes
    /// ([`reference_field`]). The floor is far above the p=1 gauge's `10⁻⁶`
    /// because the p=2 projection is a quadrature of the continuous overlap:
    /// a reference orthogonal to the continuous mode (TE₀₁ against the
    /// y-directed `sin(πx/a)` field) projects to discretization noise of
    /// `O(h⁴)`, which a `10⁻⁶` floor would let decide the sign. A matched
    /// reference overlaps at `O(1)`.
    fn gauge_p2(
        &self,
        layout: &FaceLayoutP2,
        dofs: &mut [f64],
        mode: usize,
    ) -> Result<(), PortFaceError> {
        let nodes = &self.tri_mesh.nodes;
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in nodes {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let ext = [0, 1].map(|k| (hi[k] - lo[k]).max(f64::EPSILON));
        // Quadrature samples of the mode: (weight, in-box point, e_h).
        let mut samples: Vec<(f64, [f64; 2], [f64; 2])> =
            Vec::with_capacity(layout.tris.len() * TRI_QUAD_DEG4.len());
        let mut e_norm2 = 0.0_f64;
        for (s, d) in &layout.tris {
            let c = s.map(|n| nodes[n as usize]);
            let (g, area) = tri_grads_2d(&c);
            for row in TRI_QUAD_DEG4.iter() {
                let lam = [row[0], row[1], row[2]];
                let w = row[3] * area;
                let shapes = trace_shapes_2d(&lam, &g);
                let mut e = [0.0_f64; 2];
                for (i, sh) in shapes.iter().enumerate() {
                    let v = dofs[d[i]];
                    e[0] += v * sh[0];
                    e[1] += v * sh[1];
                }
                let x = [0, 1].map(|k| {
                    let xk = lam[0] * c[0][k] + lam[1] * c[1][k] + lam[2] * c[2][k];
                    (xk - lo[k]) / ext[k]
                });
                e_norm2 += w * (e[0] * e[0] + e[1] * e[1]);
                samples.push((w, x, e));
            }
        }
        if e_norm2 <= f64::MIN_POSITIVE {
            return Ok(());
        }
        let mut best = 0.0_f64;
        for r in 0..N_REFERENCE_FIELDS {
            let (mut proj, mut f_norm2) = (0.0_f64, 0.0_f64);
            for &(w, x, e) in &samples {
                let fv = reference_field(r, x[0], x[1]);
                proj += w * (e[0] * fv[0] + e[1] * fv[1]);
                f_norm2 += w * (fv[0] * fv[0] + fv[1] * fv[1]);
            }
            let ceiling = (e_norm2 * f_norm2).sqrt();
            if ceiling > 0.0 {
                best = best.max(proj.abs() / ceiling);
                if proj.abs() > P2_GAUGE_FLOOR * ceiling {
                    if proj < 0.0 {
                        dofs.iter_mut().for_each(|v| *v = -*v);
                    }
                    return Ok(());
                }
            }
        }
        Err(PortFaceError::Modal(EigenError::UngaugableMode {
            mode,
            best_rel_proj: best,
        }))
    }

    /// Lift a face p=2 vector (the [`PortFaceModeP2::dofs`] layout) onto the
    /// full DOF vector of the **p=2** `space` (module docs: a unit-sign
    /// scatter). Off-face DOFs are zero.
    ///
    /// # Errors
    ///
    /// [`PortFaceError::EdgeNotInMesh`] / [`PortFaceError::NoAdjacentTet`]
    /// if a face edge / triangle is not an edge / face of the space's mesh.
    ///
    /// # Panics
    ///
    /// Panics if `space` is not a p=2 space or `face_dofs` has the wrong
    /// length.
    pub fn lift_p2(
        &self,
        space: &HcurlSpace,
        face_dofs: &[f64],
    ) -> Result<Vec<f64>, PortFaceError> {
        assert_eq!(space.order(), ElementOrder::P2, "lift_p2 needs a p=2 space");
        assert_eq!(face_dofs.len(), self.n_dofs_p2(), "face DOF vector length");
        let n_edges = self.edges.len();
        let mut out = vec![0.0_f64; space.n_dofs()];
        for (e, g) in self.global_edges.iter().enumerate() {
            let ge = space
                .edges()
                .binary_search(g)
                .map_err(|_| PortFaceError::EdgeNotInMesh { edge: *g })?;
            let d = space.edge_dofs(ge);
            out[d[0] as usize] = face_dofs[2 * e];
            out[d[1] as usize] = face_dofs[2 * e + 1];
        }
        for (t, tri) in self.tri_mesh.tris.iter().enumerate() {
            let mut g = tri.map(|l| self.local_to_global[l as usize]);
            g.sort_unstable();
            let gf = space
                .faces()
                .binary_search(&g)
                .map_err(|_| PortFaceError::NoAdjacentTet { index: t })?;
            let d = space.face_dofs(gf);
            out[d[0] as usize] = face_dofs[2 * n_edges + 2 * t];
            out[d[1] as usize] = face_dofs[2 * n_edges + 2 * t + 1];
        }
        Ok(out)
    }

    /// Build a [`WavePort`] on this face whose mode profiles live on the DOFs
    /// of `space`, one mode per entry of `a_inc`. The modes are the
    /// lowest-cutoff modes in ascending order.
    ///
    /// - **p=1:** exactly [`Self::wave_port`] on `space.edges()` (the mesh
    ///   edge table), so the result is bit-identical.
    /// - **p=2:** the modes of [`Self::solve_modes_p2`] lifted with
    ///   [`Self::lift_p2`].
    ///
    /// The port is a vacuum port; use [`WavePort::with_medium`] for a filled
    /// guide.
    ///
    /// # Errors
    ///
    /// As [`Self::wave_port`], plus the errors of [`Self::solve_modes_p2`]
    /// and [`Self::lift_p2`] at p=2.
    pub fn wave_port_on_space(
        &self,
        space: &HcurlSpace,
        a_inc: &[c64],
    ) -> Result<WavePort, PortFaceError> {
        if space.order() == ElementOrder::P1 {
            return self.wave_port(space.edges(), a_inc);
        }
        validate_amplitudes(a_inc)?;
        let modes = self
            .solve_modes_p2(a_inc.len())?
            .iter()
            .zip(a_inc)
            .map(|(m, &a)| {
                Ok(PortMode {
                    mode: self.lift_p2(space, &m.dofs)?,
                    k_c: m.k_c,
                    a_inc: a,
                })
            })
            .collect::<Result<Vec<_>, PortFaceError>>()?;
        Ok(WavePort {
            faces: self.faces.clone(),
            modes,
            medium: PortMedium::VACUUM,
        })
    }
}

/// Relative projection floor of the p=2 gauge (see
/// [`PortFaceProjection::solve_modes_p2`]): a reference counts only if it
/// overlaps the mode by more than 1 % of the Cauchy–Schwarz ceiling.
const P2_GAUGE_FLOOR: f64 = 1e-2;

/// Highest TE_mn index of the extended p=2 reference list.
const REF_MAX_INDEX: usize = 4;

/// The six p=1 reference fields plus one x- and one y-directed TE_mn shape
/// for every `0 ≤ m, n ≤ REF_MAX_INDEX` with a non-zero component.
const N_REFERENCE_FIELDS: usize = 6 + 2 * (REF_MAX_INDEX + 1) * (REF_MAX_INDEX + 1);

/// Reference field `r` at the in-box point `(sx, sy) ∈ [0, 1]²`.
///
/// - `r < 6`: the p=1 gauge's list, in its order: `ŷ sin(πsx)`,
///   `ŷ sin(2πsx)`, `x̂ sin(πsy)`, `x̂ sin(2πsy)`, `x̂`, `ŷ`.
/// - `r ≥ 6`: for `(m, n)` in row-major order, the TE_mn transverse shapes
///   `x̂ cos(mπsx) sin(nπsy)` then `ŷ sin(mπsx) cos(nπsy)`. A shape that
///   vanishes identically (`n = 0` for x, `m = 0` for y) projects to zero
///   and is skipped by the floor.
fn reference_field(r: usize, sx: f64, sy: f64) -> [f64; 2] {
    let pi = std::f64::consts::PI;
    match r {
        0 => [0.0, (pi * sx).sin()],
        1 => [0.0, (2.0 * pi * sx).sin()],
        2 => [(pi * sy).sin(), 0.0],
        3 => [(2.0 * pi * sy).sin(), 0.0],
        4 => [1.0, 0.0],
        5 => [0.0, 1.0],
        _ => {
            let k = r - 6;
            let (mn, comp) = (k / 2, k % 2);
            let (m, n) = (
                (mn / (REF_MAX_INDEX + 1)) as f64,
                (mn % (REF_MAX_INDEX + 1)) as f64,
            );
            if comp == 0 {
                [(m * pi * sx).cos() * (n * pi * sy).sin(), 0.0]
            } else {
                [0.0, (m * pi * sx).sin() * (n * pi * sy).cos()]
            }
        }
    }
}

/// The `a_inc` checks of [`PortFaceProjection::wave_port`].
fn validate_amplitudes(a_inc: &[c64]) -> Result<(), PortFaceError> {
    if a_inc.is_empty() {
        return Err(PortFaceError::InvalidAmplitude(
            "a wave port needs at least one mode (one a_inc entry per mode)".into(),
        ));
    }
    if let Some((m, a)) = a_inc
        .iter()
        .enumerate()
        .find(|(_, a)| !(a.re.is_finite() && a.im.is_finite()) || **a == c64::new(0.0, 0.0))
    {
        return Err(PortFaceError::InvalidAmplitude(format!(
            "mode {m} has a_inc = {a}; every mode is an S-parameter excitation and needs a \
             finite non-zero amplitude"
        )));
    }
    Ok(())
}

/// Barycentric gradients of a 2-D triangle (any winding) and its area.
fn tri_grads_2d(c: &[[f64; 2]; 3]) -> ([[f64; 2]; 3], f64) {
    let det = (c[1][0] - c[0][0]) * (c[2][1] - c[0][1]) - (c[1][1] - c[0][1]) * (c[2][0] - c[0][0]);
    let g = [
        [(c[1][1] - c[2][1]) / det, (c[2][0] - c[1][0]) / det],
        [(c[2][1] - c[0][1]) / det, (c[0][0] - c[2][0]) / det],
        [(c[0][1] - c[1][1]) / det, (c[1][0] - c[0][0]) / det],
    ];
    (g, 0.5 * det.abs())
}

/// The 8 trace-layout basis vectors in 2-D at barycentrics `lam`.
fn trace_shapes_2d(lam: &[f64; 3], g: &[[f64; 2]; 3]) -> [[f64; 2]; ND] {
    let w = |a: usize, b: usize| -> [f64; 2] {
        std::array::from_fn(|d| lam[a] * g[b][d] - lam[b] * g[a][d])
    };
    let q = |a: usize, b: usize| -> [f64; 2] {
        std::array::from_fn(|d| lam[a] * g[b][d] + lam[b] * g[a][d])
    };
    let mut out = [[0.0_f64; 2]; ND];
    for (k, (a, b)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
        out[2 * k] = w(a, b);
        out[2 * k + 1] = q(a, b);
    }
    out[6] = w(0, 1).map(|x| lam[2] * x);
    out[7] = w(1, 2).map(|x| lam[0] * x);
    out
}

/// [`super::project_port_face`] then
/// [`PortFaceProjection::wave_port_on_space`] (the order-generic
/// [`super::wave_port_from_faces`]).
///
/// # Errors
///
/// Any [`PortFaceError`] from either step.
pub fn wave_port_from_faces_on_space(
    space: &HcurlSpace,
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    a_inc: &[c64],
) -> Result<WavePort, PortFaceError> {
    super::project_port_face(mesh, faces)?.wave_port_on_space(space, a_inc)
}

/// The full-length modal flux `f = S_p · e` of a mode profile `mode` over the
/// DOFs of `space`: the p=1 Whitney kernel
/// ([`assemble_modal_flux`] on the mesh edge table) or the p=2 tangential
/// trace kernel ([`assemble_p2_surface_mass_triplets`]).
///
/// # Errors
///
/// [`DrivenError::SurfaceNotOnMesh`] at p=2 if a port triangle is not a
/// face of the space's mesh (the caller validates at p=1).
pub(crate) fn modal_flux_on_space(
    space: &HcurlSpace,
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    mode: &[f64],
) -> Result<Vec<f64>, DrivenError> {
    match space.order() {
        ElementOrder::P1 => Ok(assemble_modal_flux(mesh, faces, mode, space.edges())),
        ElementOrder::P2 => {
            let trips = assemble_p2_surface_mass_triplets(space, mesh, faces).map_err(|t| {
                DrivenError::SurfaceNotOnMesh {
                    surface: "wave port".to_string(),
                    triangle: t,
                    dangling: 1,
                    total: faces.len(),
                }
            })?;
            let mut flux = vec![0.0_f64; space.n_dofs()];
            for (r, c, v) in trips {
                flux[r] += v * mode[c];
            }
            Ok(flux)
        }
    }
}

/// The validated wave channels of `wave` on `space` (port-major,
/// mode-minor): every profile has length `space.n_dofs()`, and the fluxes
/// use [`modal_flux_on_space`]. At p=1 this is exactly
/// [`super::mixed::modal_channels`] on the mesh edge table.
pub(crate) fn modal_channels_on_space(
    space: &HcurlSpace,
    mesh: &TetMesh,
    n_lumped: usize,
    wave: &[WavePort],
) -> Result<Vec<ModalChannel>, DrivenError> {
    if space.order() == ElementOrder::P1 {
        return super::mixed::modal_channels(mesh, n_lumped, wave, space.edges());
    }
    modal_channels_with(
        mesh,
        n_lumped,
        wave,
        space.n_dofs(),
        "the p=2 DOF count",
        |port, mode| modal_flux_on_space(space, mesh, &port.faces, mode),
    )
}

/// [`super::waveguide_mode_reduce`] on the DOFs of `space`: the per-port,
/// per-mode amplitude `a_{p,m} = f_{p,m}ᵀ E` of a full-length solution `x`
/// (`space.n_dofs()`).
///
/// # Errors
///
/// [`DrivenError::SpaceMeshMismatch`] if `space` was not built on `mesh`;
/// [`DrivenError::SurfaceNotOnMesh`] for a port triangle off the mesh;
/// [`DrivenError::InvalidPort`] for a profile of the wrong length.
///
/// # Panics
///
/// Panics if `x.len() != space.n_dofs()`.
pub fn waveguide_mode_reduce_on_space(
    space: &HcurlSpace,
    mesh: &TetMesh,
    ports: &[WavePort],
    x: &[c64],
) -> Result<Vec<Vec<c64>>, DrivenError> {
    check_space(space, mesh)?;
    assert_eq!(x.len(), space.n_dofs(), "solution length != space.n_dofs()");
    let channels = modal_channels_on_space(space, mesh, 0, ports)?;
    let mut out: Vec<Vec<c64>> = ports
        .iter()
        .map(|p| Vec::with_capacity(p.n_modes()))
        .collect();
    for ch in &channels {
        let a = ch
            .flux
            .iter()
            .zip(x)
            .fold(c64::new(0.0, 0.0), |acc, (&f, &e)| acc + e * f);
        out[ch.port].push(a);
    }
    Ok(out)
}

fn check_space(space: &HcurlSpace, mesh: &TetMesh) -> Result<(), DrivenError> {
    if space.n_nodes() == mesh.n_nodes() && space.n_tets() == mesh.n_tets() {
        Ok(())
    } else {
        Err(DrivenError::SpaceMeshMismatch {
            space_nodes: space.n_nodes(),
            space_tets: space.n_tets(),
            mesh_nodes: mesh.n_nodes(),
            mesh_tets: mesh.n_tets(),
        })
    }
}

/// N-port × K-mode **wave-port** S-parameter sweep on an order-pluggable
/// [`HcurlSpace`] (issue #884): the order-generic
/// [`solve_wave_port_sweep_with_mode`].
///
/// - **p=1:** calls [`solve_wave_port_sweep_with_mode`] verbatim, so the
///   result is bit-identical to it.
/// - **p=2:** `bcs.pec_interior_mask` is over `space.n_dofs()` (build it with
///   [`HcurlSpace::pec_interior_mask`]), the port profiles are over the
///   space's DOFs ([`PortFaceProjection::wave_port_on_space`]), and the
///   S-matrix, `β` and `iters_per_rhs` have the p=1 layout and power
///   normalization. Walls go through the p=2 trace kernel.
///
/// # Errors
///
/// As [`solve_wave_port_sweep_with_mode`], plus
/// [`DrivenError::SpaceMeshMismatch`] and, at p=2, the mask / profile
/// length checks of [`DrivenOperator::assemble_with_space`] and
/// [`DrivenError::InvalidPort`].
#[allow(clippy::too_many_arguments)]
pub fn solve_wave_port_sweep_on_space<B: burn::tensor::backend::Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[WavePort],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<Vec<WavePortSweepPoint>, DrivenError> {
    check_space(space, mesh)?;
    if space.order() == ElementOrder::P1 {
        return solve_wave_port_sweep_with_mode::<B>(
            mesh,
            materials,
            sigma_tet,
            bcs,
            ports,
            surfaces,
            omegas,
            solver_mode,
            device,
        );
    }
    if ports.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "wave-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    if ports.iter().all(|p| p.modes.is_empty()) {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "wave-port S-parameter extraction needs at least one mode across all ports"
                .to_string(),
        });
    }
    let points = mixed_p2::<B>(
        space,
        mesh,
        materials,
        sigma_tet,
        bcs,
        &[],
        ports,
        surfaces,
        omegas,
        solver_mode,
        device,
    )?;
    Ok(points.into_iter().map(wave_point).collect())
}

/// A mixed point without lumped ports, in the wave-sweep layout.
fn wave_point(p: MixedPortSweepPoint) -> WavePortSweepPoint {
    WavePortSweepPoint {
        omega: p.omega,
        residual_rel: p.residual_rel,
        s: p.s,
        beta: p.beta,
        n_channels: p.n_ports,
        port_mode_counts: p.port_mode_counts,
        iters_per_rhs: p.iters_per_rhs,
    }
}

/// Mixed **lumped + wave** port sweep on an order-pluggable [`HcurlSpace`]
/// (issue #884): the order-generic
/// [`super::solve_mixed_port_sweep_with_mode`] (same power-wave S, channel
/// order and errors).
///
/// - **p=1:** calls [`super::solve_mixed_port_sweep_with_mode`] verbatim, so
///   the result is bit-identical to it.
/// - **p=2:** lumped ports, walls and the volume through
///   [`DrivenOperator::assemble_with_space`]; wave channels through the p=2
///   modal flux.
///
/// # Errors
///
/// As [`super::solve_mixed_port_sweep_with_mode`], plus
/// [`DrivenError::SpaceMeshMismatch`].
#[allow(clippy::too_many_arguments)]
pub fn solve_mixed_port_sweep_on_space<B: burn::tensor::backend::Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePort],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<Vec<MixedPortSweepPoint>, DrivenError> {
    check_space(space, mesh)?;
    if space.order() == ElementOrder::P1 {
        return super::solve_mixed_port_sweep_with_mode::<B>(
            mesh,
            materials,
            sigma_tet,
            bcs,
            lumped,
            wave,
            surfaces,
            omegas,
            solver_mode,
            device,
        );
    }
    if lumped.is_empty() && wave.is_empty() {
        return Err(DrivenError::InvalidPort {
            index: 0,
            reason: "mixed-port S-parameter extraction needs at least one port".to_string(),
        });
    }
    mixed_p2::<B>(
        space,
        mesh,
        materials,
        sigma_tet,
        bcs,
        lumped,
        wave,
        surfaces,
        omegas,
        solver_mode,
        device,
    )
}

/// The p=2 arm of the wave / mixed sweeps: the p=1 validation order, then
/// the p=2 channels, the p=2 base operator and the shared per-ω core.
#[allow(clippy::too_many_arguments)]
fn mixed_p2<B: burn::tensor::backend::Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePort],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<Vec<MixedPortSweepPoint>, DrivenError> {
    if matches!(solver_mode, SolverMode::IterativeMatrixFree(_)) {
        return Err(DrivenError::UnsupportedMatrixFree {
            reason: "wave-port sweeps (rank-N SMW modal-Robin) are not wired to the matrix-free \
                     path; use SolverMode::Direct or SolverMode::Iterative"
                .to_string(),
        });
    }
    for (index, port) in lumped.iter().enumerate() {
        if port.v_inc == c64::new(0.0, 0.0) {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "every lumped port needs a non-zero v_inc to serve as an S-parameter \
                         excitation"
                    .to_string(),
            });
        }
    }
    if bcs.pec_interior_mask.len() != space.n_dofs() {
        return Err(DrivenError::MaskDimMismatch {
            got: bcs.pec_interior_mask.len(),
            want: space.n_dofs(),
        });
    }
    // Validates the port faces (issue #725) and every profile length.
    let channels = modal_channels_on_space(space, mesh, lumped.len(), wave)?;
    let zero_source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble_with_space::<B>(
        space,
        mesh,
        materials,
        sigma_tet,
        bcs,
        lumped,
        surfaces,
        DrivenSource::Constant(&zero_source),
        device,
    )?;
    mixed_sweep_points::<B>(
        &op,
        bcs,
        lumped.len(),
        wave,
        &channels,
        space.n_dofs(),
        omegas,
        solver_mode,
        device,
    )
}

/// The geometric ports of a spec list at p=2, or the Phase 3b error for the
/// first hybrid port.
fn geometric_ports_p2(
    space: &HcurlSpace,
    wave: &[WavePortSpec],
) -> Result<Vec<WavePort>, DrivenError> {
    wave.iter()
        .map(|spec| match spec {
            WavePortSpec::Geometric(p) => Ok(p.clone()),
            WavePortSpec::Hybrid(_) => Err(DrivenError::UnsupportedAtOrder {
                order: space.order(),
                feature: "hybrid (inhomogeneous cross-section) wave ports (Epic #836 Phase 3b)",
            }),
        })
        .collect()
}

/// [`super::solve_mixed_port_spec_sweep_with_mode`] on an order-pluggable
/// [`HcurlSpace`] (issue #884).
///
/// - **p=1:** calls it verbatim, so the result is bit-identical.
/// - **p=2:** geometric ports only, through
///   [`solve_mixed_port_sweep_on_space`]. There are no hybrid reports or
///   warnings.
///
/// # Errors
///
/// [`DrivenError::UnsupportedAtOrder`] at p=2 for any hybrid port (Epic #836
/// Phase 3b); otherwise as [`solve_mixed_port_sweep_on_space`].
#[allow(clippy::too_many_arguments)]
pub fn solve_mixed_port_spec_sweep_on_space<B: burn::tensor::backend::Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    lumped: &[LumpedPort<'_>],
    wave: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<MixedPortSpecSweep, DrivenError> {
    check_space(space, mesh)?;
    if space.order() == ElementOrder::P1 {
        return super::solve_mixed_port_spec_sweep_with_mode::<B>(
            mesh,
            materials,
            sigma_tet,
            bcs,
            lumped,
            wave,
            surfaces,
            omegas,
            solver_mode,
            device,
        );
    }
    let ports = geometric_ports_p2(space, wave)?;
    let points = solve_mixed_port_sweep_on_space::<B>(
        space,
        mesh,
        materials,
        sigma_tet,
        bcs,
        lumped,
        &ports,
        surfaces,
        omegas,
        solver_mode,
        device,
    )?;
    Ok(MixedPortSpecSweep {
        points,
        hybrid: Vec::new(),
        warnings: Vec::new(),
    })
}

/// [`super::solve_wave_port_spec_sweep_with_mode`] on an order-pluggable
/// [`HcurlSpace`] (issue #884). The p=1 arm calls it verbatim; at p=2 the
/// ports must be geometric.
///
/// # Errors
///
/// As [`solve_mixed_port_spec_sweep_on_space`].
#[allow(clippy::too_many_arguments)]
pub fn solve_wave_port_spec_sweep_on_space<B: burn::tensor::backend::Backend>(
    space: &HcurlSpace,
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[WavePortSpec],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    solver_mode: SolverMode,
    device: &B::Device,
) -> Result<WavePortSpecSweep, DrivenError> {
    check_space(space, mesh)?;
    if space.order() == ElementOrder::P1 {
        return super::solve_wave_port_spec_sweep_with_mode::<B>(
            mesh,
            materials,
            sigma_tet,
            bcs,
            ports,
            surfaces,
            omegas,
            solver_mode,
            device,
        );
    }
    let geometric = geometric_ports_p2(space, ports)?;
    let points = solve_wave_port_sweep_on_space::<B>(
        space,
        mesh,
        materials,
        sigma_tet,
        bcs,
        &geometric,
        surfaces,
        omegas,
        solver_mode,
        device,
    )?;
    Ok(WavePortSpecSweep {
        points,
        hybrid: Vec::new(),
        warnings: Vec::new(),
    })
}
