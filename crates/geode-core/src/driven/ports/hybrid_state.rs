//! **Mode-tracking state machine of a real hybrid wave port** (Epic #778
//! Phase 2, issue #804; degenerate clusters and line impedances from Phase
//! 3b, issue #817): the per-port solver state a spec sweep carries from one
//! frequency to the next, and the tracking that keeps a reported channel the
//! *same* mode across the sweep.
//!
//! The entry points are [`FaceCtx`] — the static 2-D ↔ 3-D data of a face,
//! assembled once — and [`HybridState`], which owns the tracked channels,
//! the accuracy estimates and the accumulated warnings. [`PortState`] is the
//! per-port sum the sweep drivers in [`super::hybrid`] hold: a geometric
//! port's fixed fluxes, this real hybrid state, or the lossy / dispersive
//! state of [`super::hybrid_lossy`]. The tracking rules, the degenerate
//! subspace treatment and the certificate policy are documented in the
//! [`super::hybrid`] module docs; this module is where they run.
//!
//! Nothing here is part of the public surface: the reports it fills in live
//! in [`super::hybrid_report`], and only the five items the sweep drivers
//! read ([`PortState`], [`ChanAt`], [`FaceCtx`], [`HybridState`] and
//! [`check_face_matches_volume`]) are visible outside this module, plus the
//! small linear-algebra helpers [`sym_eig`] and [`complete_basis`] the lossy
//! path shares.

use std::collections::HashMap;
use std::sync::Arc;

use faer::c64;
use faer::sparse::SparseColMat;

use super::hybrid::{HybridWavePort, cluster_warning, multiplicity_warning};
use super::hybrid_lossy::LossyState;
use super::hybrid_report::{
    HybridChannelReport, HybridLineReport, HybridPortPointReport, HybridPortReport,
    PortAccuracyOpts, PortWarning, PortWarningKind,
};
use super::hybrid_z::{
    ImpedanceAccuracy, LineGeom, LineLevel, ZOutcome, ZWarnings, currents_real, line_real, line_z,
};
use super::mode_gauge::{reference_sign, whitney_samples};
use crate::analytic::port_mode_accuracy::{
    ModeAccuracy, UniformRefinement, match_refined_modes, mode_accuracy_from_matches,
    observed_rate, solve_refined,
};
use crate::analytic::port_modes::{
    DEGENERATE_REL_TOL, HybridBlocks, HybridComplexMode, HybridComplexPair, HybridPortError,
    HybridPortMode, HybridPortModeSet, HybridPortOpts, assemble_hybrid_blocks, discrete_gradient,
    sparse_matvec,
};
use crate::analytic::waveguide::TriMesh;
use crate::assembly::surface::assemble_surface_mass_triplets;
use crate::driven::solve::{DrivenError, DrivenMaterials};
use crate::mesh::TetMesh;

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// One modal channel at one frequency: sparse full-length flux, `β`,
/// admittance factor `y`, and the drive (`None`: termination only).
pub(crate) struct ChanAt {
    pub(crate) beta: c64,
    pub(crate) y: c64,
    pub(crate) a_inc: Option<c64>,
    pub(crate) flux: Vec<(usize, c64)>,
    /// The face mode behind a hybrid channel (`None`: a geometric port) —
    /// read only by the N-port S gradient through hybrid ports (Epic #841
    /// Phase 3b, issue #872); the spec sweeps ignore it.
    pub(crate) origin: Option<ChanOrigin>,
}

/// The face mode set a hybrid channel was taken from, as the tracker saw it
/// at one frequency.
#[derive(Clone)]
pub(crate) enum FaceModeSet {
    /// The real (lossless) pencil's set.
    Real(Arc<HybridPortModeSet>),
    /// The complex-symmetric (lossy / conducting / dispersive) pencil's set.
    Lossy(Arc<crate::analytic::lossy_port_modes::LossyHybridModeSet>),
}

/// Where one hybrid channel's mode came from (issue #872).
#[derive(Clone)]
pub(crate) struct ChanOrigin {
    /// The frequency's face mode set.
    pub(crate) set: FaceModeSet,
    /// The per-triangle face `ε` the set was solved with.
    pub(crate) eps: Arc<Vec<c64>>,
    /// The set-mode indices of the channel's unit: one index for a simple
    /// mode (the channel is `±` that mode), several for a direction inside
    /// an exactly degenerate cluster. Empty for a complex-pair member.
    pub(crate) members: Vec<usize>,
}

pub(super) enum PortState {
    Geometric {
        fluxes: Vec<Vec<(usize, c64)>>,
    },
    Hybrid(Box<HybridState>),
    /// A lossy / dispersive hybrid port (#806, [`super::hybrid_lossy`]).
    Lossy(Box<LossyState>),
}

/// A face built from the volume ([`HybridPortFace::tet_of_tri`] set) must
/// carry exactly the volume's per-tet `ε`, bit for bit (#806; the #815
/// review's note 2), so a lossless face on a lossy volume, or a stale fill,
/// is a loud error rather than a silently inconsistent port. A **lossy** face
/// also needs a scalar volume permittivity (anisotropic `ε` on an
/// inhomogeneous face stays out of scope, Epic #778); a real face on a
/// non-scalar volume is not checked (the Phase 2 behaviour).
///
/// [`HybridPortFace::tet_of_tri`]: super::HybridPortFace::tet_of_tri
pub(super) fn check_face_matches_volume(
    port: &HybridWavePort,
    materials: DrivenMaterials<'_>,
    index: usize,
) -> Result<(), DrivenError> {
    let face = &port.face;
    let DrivenMaterials::Scalar(vol) = materials else {
        if face.is_lossy() {
            return Err(DrivenError::InvalidPort {
                index,
                reason: "a lossy hybrid port needs a scalar (isotropic) volume permittivity; \
                         anisotropic ε on an inhomogeneous port face is not supported"
                    .to_string(),
            });
        }
        return Ok(());
    };
    let Some(tets) = &face.tet_of_tri else {
        return Ok(());
    };
    for (t, &tet) in tets.iter().enumerate() {
        let e = face
            .eps_c
            .as_ref()
            .map_or_else(|| c64::new(face.eps_r[t], 0.0), |c| c[t]);
        let v = vol.get(tet).copied();
        if v != Some(e) {
            return Err(DrivenError::InvalidPort {
                index,
                reason: format!(
                    "hybrid port face triangle {t} has ε = {e} but the tet it bounds ({tet}) has \
                     ε = {}; the port must see the volume's own permittivity — build the face \
                     from the same per-tet values (HybridPortFace::from_volume for a real fill, \
                     from_volume_lossy for a lossy one)",
                    v.map_or_else(|| "<out of range>".to_string(), |v| v.to_string())
                ),
            });
        }
    }
    Ok(())
}

/// Static 2-D ↔ 3-D data of a hybrid face.
pub(super) struct FaceCtx {
    /// 2-D edge → 3-D edge index.
    pub(super) lift: Vec<usize>,
    /// Port-face surface mass `S_p` restricted to the face edges, in 2-D
    /// edge indices.
    s_local: Vec<(usize, usize, f64)>,
    /// 2-D face Whitney mass `M₁` (tracking overlaps).
    pub(super) m1: SparseColMat<usize, f64>,
    /// 2-D discrete gradient.
    pub(super) d: SparseColMat<usize, f64>,
    /// Full hybrid blocks (`G`, `S`, `T_ε` for the conductor currents).
    pub(super) blocks: HybridBlocks,
    /// Conductor node masks (face conductor order).
    pub(super) conductors: Vec<Vec<bool>>,
    /// Voltage path of each conductor as signed 2-D edges (empty: no path).
    pub(super) paths: Vec<Vec<(usize, f64)>>,
    /// `max ε_r` of the face (the `k₀²ε_max` scale of the degeneracy test).
    pub(super) eps_max: f64,
    pub(super) h: f64,
    /// Smallest face edge (the conductor-edge cell of a graded face).
    pub(super) h_min: f64,
    /// The projected face mesh and its edge table, for the canonical mode
    /// sign at the first frequency (issue #888).
    face_mesh: TriMesh,
    face_edges: Vec<[u32; 2]>,
}

/// Solve outcome of [`FaceCtx::solve`].
pub(super) struct FaceSolve {
    pub(super) set: HybridPortModeSet,
    retried: bool,
}

impl FaceCtx {
    pub(super) fn new(
        mesh: &TetMesh,
        edges: &[[u32; 2]],
        port: &HybridWavePort,
        index: usize,
    ) -> Result<Self, DrivenError> {
        let proj = &port.face.projection;
        let lookup: HashMap<(u32, u32), usize> = edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let mut lift = Vec::with_capacity(proj.global_edges.len());
        for e in &proj.global_edges {
            let Some(&g) = lookup.get(&(e[0], e[1])) else {
                return Err(DrivenError::InvalidPort {
                    index,
                    reason: format!("hybrid port-face edge {e:?} is not an edge of the 3-D mesh"),
                });
            };
            lift.push(g);
        }
        let local: HashMap<usize, usize> = lift.iter().enumerate().map(|(l, &g)| (g, l)).collect();
        let s_local = assemble_surface_mass_triplets(mesh, &proj.faces, edges)
            .into_iter()
            .filter_map(|(r, c, v)| Some((*local.get(&r)?, *local.get(&c)?, v)))
            .collect();
        let blocks = assemble_hybrid_blocks(&proj.tri_mesh, &port.face.eps_r).map_err(|e| {
            DrivenError::InvalidPort {
                index,
                reason: format!("hybrid port-face assembly: {e}"),
            }
        })?;
        let local_edge: HashMap<(u32, u32), usize> = proj
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        let paths = port
            .face
            .conductors
            .iter()
            .map(|c| {
                c.voltage_path
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (w[0], w[1]);
                        let e = local_edge[&(a.min(b), a.max(b))];
                        (e, if a < b { 1.0 } else { -1.0 })
                    })
                    .collect()
            })
            .collect();
        Ok(Self {
            lift,
            s_local,
            m1: blocks.m1.clone(),
            d: discrete_gradient(&proj.tri_mesh),
            blocks,
            conductors: port
                .face
                .conductors
                .iter()
                .map(|c| c.nodes.clone())
                .collect(),
            paths,
            eps_max: port.face.eps_r.iter().copied().fold(1.0_f64, f64::max),
            h: port.face.mesh_size(),
            h_min: super::hybrid_z::min_edge(&proj.tri_mesh),
            face_mesh: proj.tri_mesh.clone(),
            face_edges: proj.edges.clone(),
        })
    }

    /// The canonical sign (`±1`) of a simple reported mode at the first
    /// frequency (issue #888): the reference-overlap rule of the geometric
    /// ports ([`super::reference_field`]) on the scaled transverse field
    /// `ẽ_t` (real `re`, plus `im` for a lossy mode), in the face's canonical
    /// frame. The solver's own sign (its largest edge DOF) depends on the
    /// mesh, so two ports of one guide could disagree and turn the
    /// cross-port S of the mode by 180°.
    pub(super) fn reference_sign(
        &self,
        re: &[f64],
        im: Option<&[f64]>,
        port: usize,
        mode: usize,
    ) -> Result<f64, DrivenError> {
        let mut vectors = vec![re];
        vectors.extend(im);
        let (quad, fields) = whitney_samples(&self.face_mesh, &self.face_edges, &vectors);
        reference_sign(&quad, &fields[0], fields.get(1).map(Vec::as_slice), mode).map_err(|e| {
            DrivenError::InvalidPort {
                index: port,
                reason: format!(
                    "hybrid wave port: no reference field fixes the sign of reported mode                      {mode} ({e}); its cross-port S-parameters would have a mesh-dependent sign"
                ),
            }
        })
    }

    /// Mode solve at `omega` with `extra` evanescent slots beyond the
    /// reported + termination ones; pairs carried. One retry with the bare
    /// minimum on a shortfall; one retry with twice the Krylov cap when the
    /// multiplicity pass could not certify.
    pub(super) fn solve(
        &self,
        port: &HybridWavePort,
        omega: f64,
        extra: usize,
    ) -> Result<FaceSolve, DrivenError> {
        let base = port.n_modes() + port.opts.n_termination_evanescent;
        let mut opts = HybridPortOpts {
            n_evanescent: base + extra,
            couple: true,
            deflate_null: true,
            max_krylov: port.opts.max_krylov,
            residual_tol: port.opts.residual_tol,
            carry_complex_pairs: true,
            verify_multiplicity: port.opts.verify_multiplicity,
        };
        let run = |opts: &mut HybridPortOpts| -> Result<HybridPortModeSet, HybridPortError> {
            match port.face.solve_modes(omega, opts) {
                Ok(s) => Ok(s),
                Err(HybridPortError::Shortfall { n_propagating, .. }) if extra > 0 => {
                    opts.n_evanescent = base.saturating_sub(n_propagating);
                    port.face.solve_modes(omega, opts)
                }
                Err(e) => Err(e),
            }
        };
        let err = |e: HybridPortError| {
            DrivenError::Solve(format!(
                "hybrid wave port: port-mode solve at ω = {omega} failed: {e}"
            ))
        };
        let set = run(&mut opts).map_err(err)?;
        if opts.verify_multiplicity && !set.diagnostics.multiplicity_certified {
            opts.max_krylov = 2 * opts.max_krylov.max(1);
            opts.n_evanescent = base + extra;
            // The retry is a robust path, not a requirement: keep the first
            // set if it fails.
            if let Ok(again) = run(&mut opts) {
                return Ok(FaceSolve {
                    set: again,
                    retried: true,
                });
            }
            return Ok(FaceSolve { set, retried: true });
        }
        Ok(FaceSolve {
            set,
            retried: false,
        })
    }

    /// `k₀²ε_max` at `omega`.
    fn scale(&self, omega: f64) -> f64 {
        omega * omega * self.eps_max
    }

    /// Signed discrete-Ampère conductor currents of `m` at `k0`
    /// ([`currents_real`]).
    fn currents(&self, m: &HybridPortMode, k0: f64) -> Vec<f64> {
        currents_real(&self.blocks, &self.conductors, m, k0)
    }

    /// [`Self::currents`] of a complex (lossy) mode, used only to pick the
    /// canonical basis of a degenerate cluster on a lossy face: the
    /// displacement term uses the real blocks (`T` of `Re ε`), which is exact
    /// for TEM-like clusters (`ẽ_z = 0`) and immaterial for the basis choice
    /// otherwise (any rotation of an exactly degenerate cluster is an
    /// eigenbasis).
    pub(super) fn currents_c(&self, e_t: &[c64], e_z: &[c64], k0: f64) -> Vec<c64> {
        if self.conductors.is_empty() {
            return Vec::new();
        }
        let split = |v: &[c64]| -> (Vec<f64>, Vec<f64>) {
            (
                v.iter().map(|x| x.re).collect(),
                v.iter().map(|x| x.im).collect(),
            )
        };
        let (tr, ti) = split(e_t);
        let (zr, zi) = split(e_z);
        let g = self.blocks.g.as_ref();
        let (cp, ri, v) = (g.col_ptr(), g.row_idx(), g.val());
        let mut zrow = vec![c64::new(0.0, 0.0); g.ncols()];
        for (j, o) in zrow.iter_mut().enumerate() {
            for p in cp[j]..cp[j + 1] {
                *o += c64::new(v[p] * tr[ri[p]], v[p] * ti[ri[p]]);
            }
        }
        let (sr, si) = (
            sparse_matvec(self.blocks.s.as_ref(), &zr),
            sparse_matvec(self.blocks.s.as_ref(), &zi),
        );
        let (er, ei) = (
            sparse_matvec(self.blocks.t_eps.as_ref(), &zr),
            sparse_matvec(self.blocks.t_eps.as_ref(), &zi),
        );
        let eta = crate::constants::ETA_0_OHM;
        self.conductors
            .iter()
            .map(|c| {
                let q = (0..c.len())
                    .filter(|&k| c[k])
                    .fold(c64::new(0.0, 0.0), |acc, k| {
                        acc + zrow[k] + c64::new(sr[k] - k0 * k0 * er[k], si[k] - k0 * k0 * ei[k])
                    });
                -q / (k0 * eta)
            })
            .collect()
    }

    /// [`HybridLineReport`] of a propagating mode (`None` without
    /// conductors or for `β² ≤ 0`; [`line_real`]).
    fn line(&self, m: &HybridPortMode, k0: f64) -> Option<HybridLineReport> {
        line_real(&self.blocks, &self.d, &self.conductors, &self.paths, m, k0)
    }

    /// Lift a 2-D edge vector (complex) through `S_p`: the sparse full-length
    /// flux.
    pub(super) fn flux(&self, w_local: &[c64]) -> Vec<(usize, c64)> {
        let mut f = vec![c64::new(0.0, 0.0); self.lift.len()];
        for &(r, c, v) in &self.s_local {
            f[r] += w_local[c] * v;
        }
        self.lift.iter().copied().zip(f).collect()
    }

    /// Channel data of a (sign-fixed) real mode.
    pub(super) fn real_channel(&self, m: &HybridPortMode, tripwire: bool) -> RealChannel {
        let n = m.norm;
        if tripwire {
            let mt = sparse_matvec(self.m1.as_ref(), &m.e_t);
            let nn: f64 = mt.iter().zip(&m.e_t).map(|(a, b)| a * b).sum();
            let k = 1.0 / nn.sqrt();
            let e: Vec<c64> = m.e_t.iter().map(|&v| c64::new(k * v, 0.0)).collect();
            return RealChannel {
                flux: self.flux(&e),
                e_t_local: e,
            };
        }
        // κ = 1/√N (principal root: N < 0 gives κ = −j/√|N|).
        let kappa = c64::new(n, 0.0).sqrt().inv();
        let w = transverse_pairing_from(&self.d, m);
        let e_t: Vec<c64> = m.e_t.iter().map(|&v| kappa * v).collect();
        let w_c: Vec<c64> = w.iter().map(|&v| kappa * v).collect();
        RealChannel {
            flux: self.flux(&w_c),
            e_t_local: e_t,
        }
    }

    /// Flux of one complex pair member (termination only).
    fn complex_channel(&self, z: &HybridComplexMode) -> Vec<(usize, c64)> {
        // w̃ = z_t + D z_z (complex).
        let zr: Vec<f64> = z.e_z.iter().map(|v| v.re).collect();
        let zi: Vec<f64> = z.e_z.iter().map(|v| v.im).collect();
        let dr = sparse_matvec(self.d.as_ref(), &zr);
        let di = sparse_matvec(self.d.as_ref(), &zi);
        let w: Vec<c64> = z
            .e_t
            .iter()
            .zip(dr.iter().zip(&di))
            .map(|(&t, (&a, &b))| t + c64::new(a, b))
            .collect();
        // N = z_tᵀ M₁ w̃ (= β² by the normalization; recomputed).
        let tr: Vec<f64> = z.e_t.iter().map(|v| v.re).collect();
        let ti: Vec<f64> = z.e_t.iter().map(|v| v.im).collect();
        let mr = sparse_matvec(self.m1.as_ref(), &tr);
        let mi = sparse_matvec(self.m1.as_ref(), &ti);
        let n = mr
            .iter()
            .zip(&mi)
            .zip(&w)
            .fold(c64::new(0.0, 0.0), |acc, ((&a, &b), &wv)| {
                acc + c64::new(a, b) * wv
            });
        let kappa = n.sqrt().inv();
        let w_c: Vec<c64> = w.iter().map(|&v| kappa * v).collect();
        self.flux(&w_c)
    }
}

pub(super) struct RealChannel {
    pub(super) flux: Vec<(usize, c64)>,
    pub(super) e_t_local: Vec<c64>,
}

/// `w̃ = ẽ_t + D ẽ_z` with a prebuilt `D`.
fn transverse_pairing_from(d: &SparseColMat<usize, f64>, m: &HybridPortMode) -> Vec<f64> {
    let mut w = sparse_matvec(d.as_ref(), &m.e_z);
    for (wi, ti) in w.iter_mut().zip(&m.e_t) {
        *wi += ti;
    }
    w
}

/// Pending complex-pair warning of one port.
struct PairWarn {
    mode_indices: [usize; 2],
    beta_sq: c64,
    omega: f64,
    n_omegas: usize,
    h_hint: Option<f64>,
}

impl PairWarn {
    fn into_warning(self, port: usize, h: f64) -> PortWarning {
        let hint = match self.h_hint {
            Some(hh) => format!(
                "it is predicted to resolve into two real modes near h ≈ {hh:.4e}: refine the \
                 port face below that (now {h:.4e})"
            ),
            None => format!("refine the port face (now h = {h:.4e})"),
        };
        PortWarning {
            port,
            message: format!(
                "hybrid wave port {port}: modes {}–{} form a mesh-induced complex-conjugate \
                 evanescent pair β² = {:.5} ± {:.3e}j at ω = {} ({} frequenc{}); terminated as a \
                 2×2 modal block (not reported in S). It is a discretization artifact of a \
                 near-degenerate LSE/LSM pair; {hint}",
                self.mode_indices[0],
                self.mode_indices[1],
                self.beta_sq.re,
                self.beta_sq.im.abs(),
                self.omega,
                self.n_omegas,
                if self.n_omegas == 1 { "y" } else { "ies" },
            ),
            kind: PortWarningKind::ComplexPairTerminated {
                mode_indices: self.mode_indices,
                beta_sq: self.beta_sq,
                omega: self.omega,
                n_omegas: self.n_omegas,
                h_hint: self.h_hint,
            },
        }
    }
}

pub(super) struct HybridState {
    ctx: FaceCtx,
    /// Tracked reported modes at the previous frequency (sign-fixed; inside a
    /// degenerate cluster, the tracked combinations).
    prev: Vec<HybridPortMode>,
    /// `k₀²ε_max` at the previous frequency.
    prev_scale: f64,
    refined: Option<UniformRefinement>,
    refined2: Option<UniformRefinement>,
    rates: Option<Vec<f64>>,
    /// Line operators of the `h/2` face (`Some(None)`: they failed to
    /// assemble) for the impedance estimate.
    z_h2: Option<Option<LineGeom>>,
    /// Observed per-channel impedance rates (`[Z_PI, Z_PV, Z_VI]`) at the
    /// first frequency.
    z_rates: Option<Vec<[Option<f64>; 3]>>,
    z_warn: ZWarnings,
    points: Vec<Option<HybridPortPointReport>>,
    pair_warn: Option<PairWarn>,
    dropped_warn: Vec<PortWarningKind>,
    acc_warn: Vec<Option<PortWarningKind>>,
    acc_unavail: Vec<Option<PortWarningKind>>,
    /// `(first ω, count, verified)` of uncertified multiplicity solves.
    mult_warn: Option<(f64, usize, bool)>,
    /// Cluster splits and non-canonical cluster bases.
    cluster_warn: Vec<PortWarningKind>,
}

/// An evanescent-window item of a face mode set.
enum Item<'a> {
    Real(usize, &'a HybridPortMode),
    Pair(&'a HybridComplexPair),
}

impl Item<'_> {
    fn re_beta_sq(&self) -> f64 {
        match self {
            Item::Real(_, m) => m.beta_sq,
            Item::Pair(p) => p.mode.beta_sq.re,
        }
    }
}

/// Exactly degenerate clusters of `modes` (descending `β²`): maximal runs
/// with `|β²_j − β²_first| ≤ DEGENERATE_REL_TOL · scale` and one sign of the
/// B-norm, the rule under which the P1 solver B-orthogonalizes a cluster. A
/// simple eigenvalue is a cluster of one.
fn degenerate_units(modes: &[HybridPortMode], scale: f64) -> Vec<Vec<usize>> {
    let tol = DEGENERATE_REL_TOL * scale;
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (j, m) in modes.iter().enumerate() {
        if let Some(u) = out.last_mut() {
            let f = &modes[u[0]];
            if (m.beta_sq - f.beta_sq).abs() <= tol && (m.norm > 0.0) == (f.norm > 0.0) {
                u.push(j);
                continue;
            }
        }
        out.push(vec![j]);
    }
    out
}

/// The same partition for an unordered channel list.
fn channel_groups(chs: &[HybridPortMode], scale: f64) -> Vec<Vec<usize>> {
    let tol = DEGENERATE_REL_TOL * scale;
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (i, m) in chs.iter().enumerate() {
        match out.iter_mut().find(|g| {
            let f = &chs[g[0]];
            (m.beta_sq - f.beta_sq).abs() <= tol && (m.norm > 0.0) == (f.norm > 0.0)
        }) {
            Some(g) => g.push(i),
            None => out.push(vec![i]),
        }
    }
    out
}

/// `Σ_j c_j m_j` over the members of an exactly degenerate cluster with
/// orthonormal coefficients `c` (so the B-norm is the cluster's). `β`,
/// `β²` and the norm are the first member's; the residual and its floor the
/// members' maximum; the transverse fraction the `c²`-weighted mean (exact
/// when the members are energy-orthogonal).
fn combine_modes(members: &[&HybridPortMode], c: &[f64]) -> HybridPortMode {
    let f = members[0];
    let mut out = HybridPortMode {
        beta_sq: f.beta_sq,
        beta: f.beta,
        e_t: vec![0.0; f.e_t.len()],
        e_z: vec![0.0; f.e_z.len()],
        norm: f.norm,
        residual: 0.0,
        residual_floor: 0.0,
        transverse_fraction: 0.0,
    };
    for (m, &cj) in members.iter().zip(c) {
        for (o, v) in out.e_t.iter_mut().zip(&m.e_t) {
            *o += cj * v;
        }
        for (o, v) in out.e_z.iter_mut().zip(&m.e_z) {
            *o += cj * v;
        }
        out.residual = out.residual.max(m.residual);
        out.residual_floor = out.residual_floor.max(m.residual_floor);
        out.transverse_fraction += cj * cj * m.transverse_fraction;
    }
    out
}

/// Eigen-decomposition of a small symmetric matrix (cyclic Jacobi):
/// eigenvalues descending, eigenvectors as columns `v[row][col]`.
pub(super) fn sym_eig(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len();
    let mut a: Vec<Vec<f64>> = a.to_vec();
    let mut v: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for _sweep in 0..100 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        let diag: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum();
        if off <= 1e-32 * diag.max(f64::MIN_POSITIVE) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q] == 0.0 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                // t = sgn(θ)/(|θ| + √(θ² + 1)), sgn(0) = +1.
                let sgn = if theta >= 0.0 { 1.0 } else { -1.0 };
                let t = sgn / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (akp, akq) = (row[p], row[q]);
                    row[p] = c * akp - s * akq;
                    row[q] = s * akp + c * akq;
                }
                let (rp, rq) = (a[p].clone(), a[q].clone());
                for (k, (apk, aqk)) in rp.iter().zip(&rq).enumerate() {
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| a[j][j].total_cmp(&a[i][i]));
    let vals = idx.iter().map(|&i| a[i][i]).collect();
    let vecs = (0..n)
        .map(|r| idx.iter().map(|&c| v[r][c]).collect())
        .collect();
    (vals, vecs)
}

/// Orthogonal Procrustes rotation of a degenerate cluster onto `q` previous
/// channels: `o` is the `q × g` overlap block (previous channel × cluster
/// member). Returns the `g × g` orthogonal `R` whose first `q` columns are
/// `oᵀ(ooᵀ)^{-1/2}` (so `o R[:, ..q] = (ooᵀ)^{1/2}`, symmetric positive
/// definite: the closest rotation) and whose remaining columns complete an
/// orthonormal basis (the cluster's unclaimed directions), plus the
/// principal cosines (singular values of `o`, descending).
fn procrustes(o: &[Vec<f64>], g: usize) -> (Vec<Vec<f64>>, Vec<f64>) {
    let q = o.len();
    let mut oot = vec![vec![0.0; q]; q];
    for i in 0..q {
        for k in 0..q {
            oot[i][k] = (0..g).map(|j| o[i][j] * o[k][j]).sum();
        }
    }
    let (lam, u) = sym_eig(&oot);
    let cos: Vec<f64> = lam.iter().map(|l| l.max(0.0).sqrt()).collect();
    // (ooᵀ)^{-1/2} = U Λ^{-1/2} Uᵀ (eigenvalues guarded; the caller rejects
    // a rank-deficient block before using the rotation).
    let inv_sqrt: Vec<Vec<f64>> = (0..q)
        .map(|i| {
            (0..q)
                .map(|k| {
                    (0..q)
                        .map(|m| u[i][m] * u[k][m] / lam[m].max(1e-300).sqrt())
                        .sum()
                })
                .collect()
        })
        .collect();
    let mut cols: Vec<Vec<f64>> = (0..q)
        .map(|k| {
            (0..g)
                .map(|j| (0..q).map(|i| o[i][j] * inv_sqrt[i][k]).sum())
                .collect()
        })
        .collect();
    complete_basis(&mut cols, g);
    let r = (0..g)
        .map(|j| cols.iter().map(|c| c[j]).collect())
        .collect();
    (r, cos)
}

/// Extend orthonormal columns `cols` (each of length `g`) to a full
/// orthonormal basis of `R^g` (modified Gram–Schmidt on the unit vectors).
pub(super) fn complete_basis(cols: &mut Vec<Vec<f64>>, g: usize) {
    for e in 0..g {
        if cols.len() == g {
            break;
        }
        let mut v: Vec<f64> = (0..g).map(|j| if j == e { 1.0 } else { 0.0 }).collect();
        for _ in 0..2 {
            for c in cols.iter() {
                let d: f64 = c.iter().zip(&v).map(|(a, b)| a * b).sum();
                for (vi, ci) in v.iter_mut().zip(c) {
                    *vi -= d * ci;
                }
            }
        }
        let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if n > 1e-8 {
            cols.push(v.into_iter().map(|x| x / n).collect());
        }
    }
}

/// How a face unit (a degenerate cluster, or one simple mode) is used at one
/// frequency: the orthogonal rotation of its members (columns), and how many
/// of the leading rotated directions are reported channels.
struct UnitUse {
    members: Vec<usize>,
    /// `g × g` orthogonal; `rot[j][k]` = coefficient of member `j` in
    /// direction `k`.
    rot: Vec<Vec<f64>>,
    /// `(direction k, channel)` for the claimed directions.
    claimed: Vec<(usize, usize)>,
}

impl UnitUse {
    fn identity(members: Vec<usize>) -> Self {
        let g = members.len();
        Self {
            rot: (0..g)
                .map(|i| (0..g).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
                .collect(),
            members,
            claimed: Vec::new(),
        }
    }

    /// The mode along rotated direction `k`.
    fn direction(&self, modes: &[HybridPortMode], k: usize) -> HybridPortMode {
        if self.members.len() == 1 {
            let mut m = modes[self.members[0]].clone();
            if self.rot[0][0] < 0.0 {
                m.e_t.iter_mut().for_each(|v| *v = -*v);
                m.e_z.iter_mut().for_each(|v| *v = -*v);
            }
            return m;
        }
        let mem: Vec<&HybridPortMode> = self.members.iter().map(|&j| &modes[j]).collect();
        let c: Vec<f64> = (0..self.members.len()).map(|j| self.rot[j][k]).collect();
        combine_modes(&mem, &c)
    }
}

impl HybridState {
    /// The port's report and warnings at the end of the sweep.
    pub(super) fn into_report(self, p_idx: usize) -> (HybridPortReport, Vec<PortWarning>) {
        let mut warnings = Vec::new();
        let h = self.ctx.h;
        if let Some(w) = self.pair_warn {
            warnings.push(w.into_warning(p_idx, h));
        }
        warnings.extend(self.dropped_warn.into_iter().map(|kind| {
            let PortWarningKind::ComplexPairDropped {
                mode_indices,
                omega,
                unfilled_slots,
            } = &kind
            else {
                unreachable!()
            };
            PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx}: complex pair (modes {}–{}) at ω = {omega} has a \
                     degenerate self-pairing zᵀBz ≈ 0 and cannot be terminated as a 2×2 \
                     block; the evanescent termination window stops above it \
                     ({unfilled_slots} slot(s) unfilled). Refine the port face below h = \
                     {h:.4e}",
                    mode_indices[0], mode_indices[1]
                ),
                kind,
            }
        }));
        for kind in self.acc_warn.into_iter().flatten() {
            let PortWarningKind::AccuracyAboveThreshold {
                channel,
                omega,
                estimate,
                threshold,
                h,
                h_required,
                ez_energy_fraction,
            } = &kind
            else {
                unreachable!()
            };
            warnings.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: estimated β error {:.3} % at \
                     ω = {omega} exceeds {:.3} % (E_z energy fraction {:.2}); refine the port \
                     face to h ≤ {h_required:.4e} (now {h:.4e})",
                    100.0 * estimate,
                    100.0 * threshold,
                    ez_energy_fraction
                ),
                kind,
            });
        }
        for kind in self.acc_unavail.into_iter().flatten() {
            let PortWarningKind::AccuracyUnavailable { channel, omega } = &kind else {
                unreachable!()
            };
            warnings.push(PortWarning {
                port: p_idx,
                message: format!(
                    "hybrid wave port {p_idx} channel {channel}: no refined (h/2) counterpart \
                     matched (or the refined solve failed) at ω = {omega}; accuracy estimate \
                     unavailable"
                ),
                kind,
            });
        }
        warnings.extend(self.z_warn.into_warnings(p_idx));
        if let Some((omega, n_omegas, verified)) = self.mult_warn {
            warnings.push(multiplicity_warning(p_idx, omega, n_omegas, verified));
        }
        warnings.extend(
            self.cluster_warn
                .into_iter()
                .map(|kind| cluster_warning(p_idx, kind)),
        );
        let report = HybridPortReport {
            port: p_idx,
            n_conductors: self.ctx.conductors.len(),
            mesh_size: h,
            points: self
                .points
                .into_iter()
                .map(|p| p.expect("every frequency visited"))
                .collect(),
            observed_rates: self.rates,
        };
        (report, warnings)
    }

    pub(super) fn new(ctx: FaceCtx, n_omegas: usize, k: usize) -> Self {
        Self {
            ctx,
            prev: Vec::new(),
            prev_scale: 0.0,
            mult_warn: None,
            cluster_warn: Vec::new(),
            refined: None,
            refined2: None,
            rates: None,
            z_h2: None,
            z_rates: None,
            z_warn: ZWarnings::new(k),
            points: vec![None; n_omegas],
            pair_warn: None,
            dropped_warn: Vec::new(),
            acc_warn: vec![None; k],
            acc_unavail: vec![None; k],
        }
    }

    /// Solve, track and lift one hybrid port at `omega`; returns the
    /// reported and the termination channels.
    pub(super) fn channels_at(
        &mut self,
        port: &HybridWavePort,
        p_idx: usize,
        omega: f64,
        input_index: usize,
        first: bool,
    ) -> Result<(Vec<ChanAt>, Vec<ChanAt>), DrivenError> {
        let k = port.n_modes();
        let FaceSolve { set, retried } = self.ctx.solve(port, omega, 2)?;
        let set = Arc::new(set);
        let face_eps: Arc<Vec<c64>> =
            Arc::new(port.face.eps_r.iter().map(|&e| c64::new(e, 0.0)).collect());
        let origin = |members: Vec<usize>| ChanOrigin {
            set: FaceModeSet::Real(Arc::clone(&set)),
            eps: Arc::clone(&face_eps),
            members,
        };
        let n_prop = set.n_propagating;
        let scale = self.ctx.scale(omega);
        if !set.diagnostics.multiplicity_certified {
            match &mut self.mult_warn {
                Some(w) => w.1 += 1,
                None => self.mult_warn = Some((omega, 1, port.opts.verify_multiplicity)),
            }
        }

        // Units: exactly degenerate clusters of the real modes.
        let unit_members = degenerate_units(&set.modes, scale);
        let mut unit_of = vec![0usize; set.modes.len()];
        for (u, mem) in unit_members.iter().enumerate() {
            for &j in mem {
                unit_of[j] = u;
            }
        }
        let mut units: Vec<UnitUse> = unit_members.into_iter().map(UnitUse::identity).collect();

        // All items in descending Re β².
        let mut items: Vec<Item<'_>> = set
            .modes
            .iter()
            .enumerate()
            .map(|(i, m)| Item::Real(i, m))
            .chain(set.complex_pairs.iter().map(Item::Pair))
            .collect();
        items.sort_by(|a, b| b.re_beta_sq().total_cmp(&a.re_beta_sq()));
        // Mode index of each item (pairs take two).
        let mut idx_of = Vec::with_capacity(items.len());
        let mut acc = 0usize;
        for it in &items {
            idx_of.push(acc);
            acc += if matches!(it, Item::Pair(_)) { 2 } else { 1 };
        }

        // --- Reported channels: first-ω selection or tracking.
        let mut overlaps: Vec<Option<f64>> = vec![None; k];
        if first || self.prev.is_empty() {
            let mut taken = 0usize;
            for (pos, it) in items.iter().enumerate() {
                if taken >= k {
                    break;
                }
                match it {
                    Item::Real(i, _) => {
                        let u = unit_of[*i];
                        if !units[u].claimed.is_empty() {
                            continue;
                        }
                        let g = units[u].members.len();
                        if taken + g > k {
                            return Err(DrivenError::InvalidPort {
                                index: p_idx,
                                reason: format!(
                                    "hybrid wave port: at ω = {omega} the last {} of the {k} \
                                     reported channel(s) would split an exactly degenerate \
                                     cluster of {g} modes (β² = {:.6}); a channel inside a \
                                     degenerate eigenspace is not uniquely defined — report \
                                     {taken} or {} channel(s)",
                                    k - taken,
                                    set.modes[*i].beta_sq,
                                    taken + g
                                ),
                            });
                        }
                        units[u].claimed = (0..g).map(|d| (d, taken + d)).collect();
                        if g > 1 {
                            self.canonicalize(&set.modes, &mut units[u], omega);
                        } else {
                            units[u].rot[0][0] =
                                self.ctx
                                    .reference_sign(&set.modes[*i].e_t, None, p_idx, taken)?;
                        }
                        taken += g;
                    }
                    Item::Pair(p) => {
                        return Err(DrivenError::InvalidPort {
                            index: p_idx,
                            reason: format!(
                                "hybrid wave port: reported channel {pos} at ω = {omega} is a \
                                 mesh-induced complex-conjugate pair (β² = {:.5} ± {:.3e}j); a \
                                 pair carries no power and cannot be an S-parameter channel — \
                                 report at most {pos} channel(s) or refine the port face \
                                 (h = {:.4e})",
                                p.mode.beta_sq.re,
                                p.mode.beta_sq.im.abs(),
                                self.ctx.h
                            ),
                        });
                    }
                }
            }
            if taken < k {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: only {taken} real modes at ω = {omega}, {k} \
                     channels requested"
                )));
            }
        } else {
            self.track(
                &set,
                &mut units,
                &mut overlaps,
                p_idx,
                omega,
                port.opts.min_track_overlap,
            )?;
        }

        // Completeness: every propagating mode is a reported channel (a
        // degenerate cluster counts once all its directions are claimed).
        for (i, m) in set.modes.iter().enumerate().take(n_prop) {
            let u = &units[unit_of[i]];
            if u.claimed.len() < u.members.len() {
                return Err(DrivenError::InvalidPort {
                    index: p_idx,
                    reason: format!(
                        "hybrid wave port: at ω = {omega} the port face carries {n_prop} \
                         propagating mode(s) but only {k} reported channel(s); the mode with \
                         β = {:.6} (E_z energy fraction {:.2}) would be unterminated and S \
                         silently wrong — raise the port's mode count",
                        m.beta.re,
                        1.0 - m.transverse_fraction
                    ),
                });
            }
        }

        // Tracked reported modes in channel order.
        let mut tracked_opt: Vec<Option<(HybridPortMode, usize)>> = vec![None; k];
        let mut chan_members: Vec<Vec<usize>> = vec![Vec::new(); k];
        for u in &units {
            for &(dir, ch) in &u.claimed {
                tracked_opt[ch] = Some((u.direction(&set.modes, dir), u.members.len()));
                chan_members[ch].clone_from(&u.members);
            }
        }
        let (tracked, cluster_sizes): (Vec<HybridPortMode>, Vec<usize>) = tracked_opt
            .into_iter()
            .map(|t| t.expect("every channel assigned"))
            .unzip();
        let mut reported = Vec::with_capacity(k);
        for ((m, &a), members) in tracked.iter().zip(&port.a_inc).zip(chan_members) {
            let ch = self.ctx.real_channel(m, port.opts.transverse_only_flux);
            reported.push(ChanAt {
                beta: m.beta,
                y: m.beta,
                a_inc: Some(a),
                flux: ch.flux,
                origin: Some(origin(members)),
            });
        }

        // --- Termination window (the unclaimed directions of each unit).
        let want = port.opts.n_termination_evanescent;
        let mut termination = Vec::new();
        let (mut slots, mut term_real, mut term_pairs) = (0usize, 0usize, 0usize);
        let mut unit_done = vec![false; units.len()];
        for (pos, it) in items.iter().enumerate() {
            if slots >= want {
                break;
            }
            match it {
                Item::Real(i, _) => {
                    let ui = unit_of[*i];
                    if unit_done[ui] {
                        continue;
                    }
                    unit_done[ui] = true;
                    let u = &units[ui];
                    for dir in u.claimed.len()..u.members.len() {
                        let m = u.direction(&set.modes, dir);
                        let ch = self.ctx.real_channel(&m, port.opts.transverse_only_flux);
                        termination.push(ChanAt {
                            beta: m.beta,
                            y: m.beta,
                            a_inc: None,
                            flux: ch.flux,
                            origin: Some(origin(u.members.clone())),
                        });
                        slots += 1;
                        term_real += 1;
                    }
                }
                Item::Pair(p) => {
                    let mi = [idx_of[pos], idx_of[pos] + 1];
                    if p.degenerate || p.conditioning <= port.opts.pair_degenerate_tol {
                        self.dropped_warn.push(PortWarningKind::ComplexPairDropped {
                            mode_indices: mi,
                            omega,
                            unfilled_slots: want - slots,
                        });
                        break;
                    }
                    for z in p.members() {
                        termination.push(ChanAt {
                            beta: z.beta,
                            y: z.beta,
                            a_inc: None,
                            flux: self.ctx.complex_channel(&z),
                            origin: Some(origin(Vec::new())),
                        });
                    }
                    slots += 2;
                    term_pairs += 1;
                    match &mut self.pair_warn {
                        Some(w) => w.n_omegas += 1,
                        None => {
                            self.pair_warn = Some(PairWarn {
                                mode_indices: mi,
                                beta_sq: p.mode.beta_sq,
                                omega,
                                n_omegas: 1,
                                h_hint: None,
                            });
                        }
                    }
                }
            }
        }

        // --- Line quantities and the accuracy estimate.
        let lines: Vec<Option<HybridLineReport>> =
            tracked.iter().map(|m| self.ctx.line(m, omega)).collect();
        for (c, l) in lines.iter().enumerate() {
            if l.as_ref().is_some_and(|l| l.no_net_current) {
                self.z_warn.record_no_current(c, omega);
            }
        }
        let mut accuracy: Vec<Option<ModeAccuracy>> = vec![None; k];
        let mut z_acc: Vec<Option<ImpedanceAccuracy>> = vec![None; k];
        if let Some(acc) = port.opts.accuracy {
            let z_out;
            (accuracy, z_out) =
                self.estimate(port, &tracked, &lines, &cluster_sizes, omega, first, acc);
            for (c, z) in z_out.iter().enumerate() {
                self.z_warn
                    .record(c, omega, z, &acc, self.ctx.h, self.ctx.h_min);
                z_acc[c] = z.accuracy();
            }
            for (c, a) in accuracy.iter().enumerate() {
                match a {
                    None => {
                        if self.acc_unavail[c].is_none() {
                            self.acc_unavail[c] =
                                Some(PortWarningKind::AccuracyUnavailable { channel: c, omega });
                        }
                    }
                    Some(a) if tracked[c].beta_sq > 0.0 && a.estimate > acc.threshold => {
                        let worse = match &self.acc_warn[c] {
                            Some(PortWarningKind::AccuracyAboveThreshold { estimate, .. }) => {
                                a.estimate > *estimate
                            }
                            _ => true,
                        };
                        if worse {
                            self.acc_warn[c] = Some(PortWarningKind::AccuracyAboveThreshold {
                                channel: c,
                                omega,
                                estimate: a.estimate,
                                threshold: acc.threshold,
                                h: self.ctx.h,
                                h_required: a.required_h(self.ctx.h, acc.threshold),
                                ez_energy_fraction: a.ez_energy_fraction,
                            });
                        }
                    }
                    Some(_) => {}
                }
            }
        }

        let tol = port.opts.residual_tol;
        let floor_accepted = set
            .modes
            .iter()
            .filter(|m| m.residual > tol && m.residual <= m.residual_floor)
            .count();
        self.points[input_index] = Some(HybridPortPointReport {
            omega,
            channels: tracked
                .iter()
                .zip(&overlaps)
                .zip(accuracy)
                .zip(&cluster_sizes)
                .zip(lines.into_iter().zip(z_acc))
                .map(|((((m, &ov), a), &g), (line, za))| HybridChannelReport {
                    beta: m.beta,
                    beta_sq: m.beta_sq,
                    beta_sq_im: 0.0,
                    ez_energy_fraction: 1.0 - m.transverse_fraction,
                    track_overlap: ov,
                    accuracy: a,
                    residual: m.residual,
                    alpha_accuracy: None,
                    impedance_accuracy: za,
                    residual_floor: m.residual_floor,
                    floor_accepted: m.residual > tol && m.residual <= m.residual_floor,
                    cluster_size: g,
                    line,
                    line_lossy: None,
                })
                .collect(),
            n_propagating: n_prop,
            termination_real: term_real,
            termination_pairs: term_pairs,
            multiplicity_certified: set.diagnostics.multiplicity_certified,
            multiplicity_retried: retried,
            repeated_copies: set.diagnostics.repeated_copies,
            degenerate_clusters: set.diagnostics.degenerate_clusters,
            floor_accepted,
        });
        self.prev = tracked;
        self.prev_scale = scale;
        Ok((reported, termination))
    }

    /// Canonical basis of a reported degenerate cluster at the first
    /// frequency: rotate so the members' conductor-current vectors are
    /// mutually orthogonal (the SVD of the `g × n_c` current matrix `Q`,
    /// directions by descending current norm; even / odd modes of a
    /// symmetric pair), each signed so its first conductor carrying at least
    /// half the largest current has positive current. Without enough
    /// conductors, or when two singular values coincide (uncoupled lines),
    /// the solver's basis is kept and a warning is recorded.
    fn canonicalize(&mut self, modes: &[HybridPortMode], unit: &mut UnitUse, omega: f64) {
        let g = unit.members.len();
        let channels: Vec<usize> = unit.claimed.iter().map(|&(_, c)| c).collect();
        let n_c = self.ctx.conductors.len();
        let fail = |this: &mut Self, reason: String| {
            this.cluster_warn
                .push(PortWarningKind::NonCanonicalClusterBasis {
                    channels: channels.clone(),
                    omega,
                    reason,
                });
        };
        if n_c < g {
            fail(
                self,
                format!("{g} degenerate modes but {n_c} floating conductor(s)"),
            );
            return;
        }
        let q: Vec<Vec<f64>> = unit
            .members
            .iter()
            .map(|&j| self.ctx.currents(&modes[j], omega))
            .collect();
        let qqt: Vec<Vec<f64>> = (0..g)
            .map(|a| {
                (0..g)
                    .map(|b| (0..n_c).map(|c| q[a][c] * q[b][c]).sum())
                    .collect()
            })
            .collect();
        let (lam, u) = sym_eig(&qqt);
        let top = lam[0].max(f64::MIN_POSITIVE);
        let min_gap = lam
            .windows(2)
            .map(|w| (w[0] - w[1]) / top)
            .fold(f64::INFINITY, f64::min);
        if lam[g - 1] <= 1e-12 * top || min_gap <= 1e-6 {
            fail(
                self,
                format!(
                    "conductor currents do not separate the modes (relative singular-value \
                     gap {min_gap:.1e}, smallest {:.1e})",
                    (lam[g - 1] / top).max(0.0).sqrt()
                ),
            );
            return;
        }
        let mut rot = u;
        for kcol in 0..g {
            let cur: Vec<f64> = (0..n_c)
                .map(|c| (0..g).map(|j| rot[j][kcol] * q[j][c]).sum())
                .collect();
            let big = cur.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
            let lead = cur
                .iter()
                .find(|v| v.abs() >= 0.5 * big)
                .copied()
                .unwrap_or(1.0);
            if lead < 0.0 {
                for row in rot.iter_mut() {
                    row[kcol] = -row[kcol];
                }
            }
        }
        unit.rot = rot;
    }

    /// Track the previous channels onto the units of `set` (module docs):
    /// greedy one-to-one at the unit level, verified per previous cluster by
    /// principal angles; degenerate units are rotated onto their claimed
    /// channels (orthogonal Procrustes).
    #[allow(clippy::too_many_arguments)]
    fn track(
        &mut self,
        set: &HybridPortModeSet,
        units: &mut [UnitUse],
        overlaps: &mut [Option<f64>],
        p_idx: usize,
        omega: f64,
        min_overlap: f64,
    ) -> Result<(), DrivenError> {
        let w_new: Vec<Vec<f64>> = set
            .modes
            .iter()
            .map(|m| transverse_pairing_from(&self.ctx.d, m))
            .collect();
        let dotf = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        let n_prev = self.prev.len();
        let n_new = set.modes.len();
        let n_units = units.len();
        // Signed normalized B-pairing overlaps.
        let mut o = vec![vec![0.0_f64; n_new]; n_prev];
        let mut pair_o = vec![0.0_f64; n_prev];
        let mut m1e = Vec::with_capacity(n_prev);
        for (i, pm) in self.prev.iter().enumerate() {
            let me = sparse_matvec(self.ctx.m1.as_ref(), &pm.e_t);
            for (j, (nm, w)) in set.modes.iter().zip(&w_new).enumerate() {
                o[i][j] = dotf(&me, w) / (pm.norm.abs() * nm.norm.abs()).sqrt();
            }
            for p in &set.complex_pairs {
                let z = &p.mode;
                let zr: Vec<f64> = z.e_z.iter().map(|v| v.re).collect();
                let zi: Vec<f64> = z.e_z.iter().map(|v| v.im).collect();
                let dr = sparse_matvec(self.ctx.d.as_ref(), &zr);
                let di = sparse_matvec(self.ctx.d.as_ref(), &zi);
                let re: f64 = me
                    .iter()
                    .zip(z.e_t.iter().zip(&dr))
                    .map(|(a, (t, d))| a * (t.re + d))
                    .sum();
                let im: f64 = me
                    .iter()
                    .zip(z.e_t.iter().zip(&di))
                    .map(|(a, (t, d))| a * (t.im + d))
                    .sum();
                let v =
                    (2.0 * (re * re + im * im)).sqrt() / (pm.norm.abs() * z.beta_sq.norm()).sqrt();
                pair_o[i] = pair_o[i].max(v);
            }
            m1e.push(me);
        }
        // Captured overlap of each previous channel by each unit.
        let cap: Vec<Vec<f64>> = (0..n_prev)
            .map(|i| {
                units
                    .iter()
                    .map(|u| {
                        u.members
                            .iter()
                            .map(|&j| o[i][j] * o[i][j])
                            .sum::<f64>()
                            .sqrt()
                    })
                    .collect()
            })
            .collect();
        // Greedy one-to-one assignment with unit capacity.
        let mut assign = vec![usize::MAX; n_prev];
        let mut load = vec![0usize; n_units];
        let mut cand: Vec<(f64, usize, usize)> = (0..n_prev)
            .flat_map(|i| (0..n_units).map(move |u| (i, u)))
            .map(|(i, u)| (cap[i][u], i, u))
            .collect();
        cand.sort_by(|a, b| b.0.total_cmp(&a.0));
        for &(_, i, u) in &cand {
            if assign[i] == usize::MAX && load[u] < units[u].members.len() {
                assign[i] = u;
                load[u] += 1;
            }
        }
        if assign.contains(&usize::MAX) {
            return Err(DrivenError::Solve(format!(
                "hybrid wave port {p_idx}: fewer real modes than channels at ω = {omega}; mode \
                 identity lost — refine the sweep"
            )));
        }
        // Verify per previous cluster.
        for grp in channel_groups(&self.prev, self.prev_scale) {
            let runner_up = |i: usize, own: &[usize]| {
                (0..n_units)
                    .filter(|u| !own.contains(u))
                    .map(|u| cap[i][u])
                    .fold(0.0_f64, f64::max)
            };
            for &i in &grp {
                if pair_o[i] >= min_overlap && pair_o[i] > cap[i][assign[i]] {
                    return Err(DrivenError::Solve(format!(
                        "hybrid wave port {p_idx}: channel {i} (β = {:.6}) collided into a \
                         mesh-induced complex-conjugate pair at ω = {omega}; a reported channel \
                         cannot be a pair — refine the port face (h = {:.4e}) or report fewer \
                         channels",
                        self.prev[i].beta, self.ctx.h
                    )));
                }
            }
            if grp.len() == 1 {
                let i = grp[0];
                let (ov, second) = (cap[i][assign[i]], runner_up(i, &[assign[i]]));
                if ov < min_overlap || second >= min_overlap {
                    return Err(DrivenError::Solve(format!(
                        "hybrid wave port {p_idx}: mode identity lost for channel {i} between \
                         the previous frequency and ω = {omega} (best overlap {ov:.3}, \
                         runner-up {second:.3}, threshold {min_overlap}); modes of the same \
                         family cross or mix here — refine the sweep"
                    )));
                }
                continue;
            }
            // A previous degenerate cluster: its subspace must be captured by
            // the units it was assigned to.
            let mut own: Vec<usize> = grp.iter().map(|&i| assign[i]).collect();
            own.sort_unstable();
            own.dedup();
            let cols: Vec<usize> = own
                .iter()
                .flat_map(|&u| units[u].members.iter().copied())
                .collect();
            let block: Vec<Vec<f64>> = grp
                .iter()
                .map(|&i| cols.iter().map(|&j| o[i][j]).collect())
                .collect();
            let (_, cos) = procrustes(&block, cols.len());
            let min_cos = cos[cos.len() - 1];
            let second = grp
                .iter()
                .map(|&i| runner_up(i, &own))
                .fold(0.0_f64, f64::max);
            if min_cos < min_overlap || second >= min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: mode identity lost for the degenerate cluster of \
                     channels {grp:?} between the previous frequency and ω = {omega} (smallest \
                     principal cosine {min_cos:.3}, runner-up {second:.3}, threshold \
                     {min_overlap}); refine the sweep"
                )));
            }
            if own.len() > 1 {
                self.cluster_warn.push(PortWarningKind::ClusterSplit {
                    channels: grp.clone(),
                    omega,
                    min_cosine: min_cos,
                });
            }
        }
        // Rotate / sign each claimed unit.
        for (u, unit) in units.iter_mut().enumerate() {
            let claimed: Vec<usize> = (0..n_prev).filter(|&i| assign[i] == u).collect();
            if claimed.is_empty() {
                continue;
            }
            if unit.members.len() == 1 {
                let (i, j) = (claimed[0], unit.members[0]);
                let sgn = dotf(&m1e[i], &set.modes[j].e_t);
                unit.rot[0][0] = if sgn < 0.0 { -1.0 } else { 1.0 };
                unit.claimed = vec![(0, i)];
                overlaps[i] = Some(o[i][j].abs());
                continue;
            }
            let g = unit.members.len();
            let block: Vec<Vec<f64>> = claimed
                .iter()
                .map(|&i| unit.members.iter().map(|&j| o[i][j]).collect())
                .collect();
            let (rot, cos) = procrustes(&block, g);
            let min_cos = cos[cos.len() - 1];
            if min_cos < min_overlap {
                return Err(DrivenError::Solve(format!(
                    "hybrid wave port {p_idx}: channels {claimed:?} map onto one degenerate \
                     cluster at ω = {omega} with smallest principal cosine {min_cos:.3} \
                     (threshold {min_overlap}); mode identity lost — refine the sweep"
                )));
            }
            for (kdir, &i) in claimed.iter().enumerate() {
                let ov: f64 = (0..g).map(|j| block[kdir][j] * rot[j][kdir]).sum();
                overlaps[i] = Some(ov.abs());
            }
            unit.rot = rot;
            unit.claimed = claimed.iter().enumerate().map(|(d, &i)| (d, i)).collect();
        }
        Ok(())
    }

    /// Per-channel accuracy estimate at `omega` (and the observed rates at
    /// the first frequency), with the line-impedance estimate of every
    /// channel that has a line (`lines`). A failed refined (`h/2`) solve
    /// gives `None` for every channel (reported as
    /// [`PortWarningKind::AccuracyUnavailable`] and, for the impedances,
    /// [`PortWarningKind::ImpedanceAccuracyUnavailable`]), a failed `h/4`
    /// rate solve the nominal `β` rate and the singular impedance rate: the
    /// estimate is a diagnostic and never aborts the sweep (the #815
    /// review's first follow-up note; the lossy path already behaves this
    /// way).
    #[allow(clippy::too_many_arguments)]
    fn estimate(
        &mut self,
        port: &HybridWavePort,
        tracked: &[HybridPortMode],
        lines: &[Option<HybridLineReport>],
        cluster_sizes: &[usize],
        omega: f64,
        first: bool,
        acc: PortAccuracyOpts,
    ) -> (Vec<Option<ModeAccuracy>>, Vec<ZOutcome>) {
        let unavailable = |why: &str| -> Vec<ZOutcome> {
            lines
                .iter()
                .map(|l| match l {
                    Some(l) if !l.no_net_current => ZOutcome::Unavailable(why.to_string()),
                    _ => ZOutcome::NotApplicable,
                })
                .collect()
        };
        let want_z = lines.iter().any(Option::is_some);
        let refined = self.refined.get_or_insert_with(|| port.face.refine());
        let n_ev_tracked = tracked.iter().filter(|m| m.beta_sq < 0.0).count();
        let opts = HybridPortOpts {
            n_evanescent: n_ev_tracked + 2,
            max_krylov: port.opts.max_krylov.max(4 * 48),
            residual_tol: port.opts.residual_tol,
            ..HybridPortOpts::default()
        };
        let Ok(set_h2) = solve_refined(refined, omega, &opts) else {
            return (
                vec![None; tracked.len()],
                unavailable("the refined (h/2) solve failed"),
            );
        };
        let coarse: Vec<&HybridPortMode> = tracked.iter().collect();
        let matched_h2 = match_refined_modes(refined, &coarse, &set_h2);
        let face_mesh = &port.face.projection.tri_mesh;
        if want_z && self.z_h2.is_none() {
            let level =
                LineLevel::of_face(&port.face).refine(face_mesh, &port.face.interior_edge_mask);
            self.z_h2 = Some(LineGeom::new(&refined.mesh, &refined.eps_r, &level));
        }
        if first && acc.observe_rate && self.rates.is_none() {
            let base = mode_accuracy_from_matches(&coarse, &set_h2, &matched_h2, None);
            let refined2 = self.refined2.get_or_insert_with(|| {
                UniformRefinement::new(
                    &refined.mesh,
                    &refined.eps_r,
                    &refined.interior_edge_mask,
                    &refined.free_node_mask,
                )
            });
            let nominal = crate::analytic::port_mode_accuracy::NOMINAL_RATE;
            let mut z_rates = vec![[None; 3]; tracked.len()];
            let rates = match solve_refined(refined2, omega, &opts) {
                Ok(set_h4) => {
                    let mid: Vec<&HybridPortMode> = matched_h2
                        .iter()
                        .filter_map(|m| m.map(|(j, _)| &set_h2.modes[j]))
                        .collect();
                    let fine = match_refined_modes(refined2, &mid, &set_h4);
                    let z_h4 = want_z
                        .then(|| {
                            let level = LineLevel::of_face(&port.face)
                                .refine(face_mesh, &port.face.interior_edge_mask)
                                .refine(&refined.mesh, &refined.interior_edge_mask);
                            LineGeom::new(&refined2.mesh, &refined2.eps_r, &level)
                        })
                        .flatten();
                    let mut rates = Vec::with_capacity(tracked.len());
                    let mut fi = fine.into_iter();
                    for (c, b) in base.iter().enumerate() {
                        let r = match (b, matched_h2[c]) {
                            (Some(b), Some((j2, _))) => match fi.next().flatten() {
                                Some((j4, _)) => {
                                    let m4 = &set_h4.modes[j4];
                                    if let (Some(l1), Some(Some(g2)), Some(g4)) =
                                        (&lines[c], &self.z_h2, &z_h4)
                                        && let (Some(l2), Some(l4)) =
                                            (g2.line(&set_h2.modes[j2], omega), g4.line(m4, omega))
                                    {
                                        z_rates[c] = super::hybrid_z::observed_rates(
                                            &line_z(l1),
                                            &line_z(&l2),
                                            &line_z(&l4),
                                        );
                                    }
                                    observed_rate(tracked[c].beta, b.beta_refined, m4.beta)
                                }
                                None => nominal,
                            },
                            _ => nominal,
                        };
                        rates.push(r);
                    }
                    rates
                }
                // The rate is a refinement of the estimate, not a
                // requirement: fall back to the nominal rate.
                Err(_) => vec![nominal; tracked.len()],
            };
            self.rates = Some(rates);
            self.z_rates = Some(z_rates);
            // Pair hint: linear extrapolation of |Im β²| over h, h/2.
            if let Some(w) = &mut self.pair_warn
                && w.h_hint.is_none()
            {
                w.h_hint = pair_hint(w.beta_sq, &set_h2, self.ctx.h);
            }
        }
        if let Some(w) = &mut self.pair_warn
            && w.h_hint.is_none()
            && (w.omega - omega).abs() <= f64::EPSILON * omega.abs()
        {
            w.h_hint = pair_hint(w.beta_sq, &set_h2, self.ctx.h);
        }
        let beta_acc =
            mode_accuracy_from_matches(&coarse, &set_h2, &matched_h2, self.rates.as_deref());
        let z = (0..tracked.len())
            .map(|c| {
                // No net conductor current (#953): no line impedance.
                let Some(l1) = lines[c].as_ref().filter(|l| !l.no_net_current) else {
                    return ZOutcome::NotApplicable;
                };
                let Some(Some(g2)) = &self.z_h2 else {
                    return ZOutcome::Unavailable(
                        "the refined (h/2) line operators did not assemble".into(),
                    );
                };
                if cluster_sizes[c] > 1 {
                    return ZOutcome::Unavailable(format!(
                        "the channel is one of an exactly degenerate cluster of {} modes, whose \
                         refined counterpart is not unique",
                        cluster_sizes[c]
                    ));
                }
                let Some((j, _)) = matched_h2[c] else {
                    return ZOutcome::Unavailable("no refined (h/2) counterpart matched".into());
                };
                let Some(l2) = g2.line(&set_h2.modes[j], omega) else {
                    return ZOutcome::Unavailable(
                        "the refined (h/2) counterpart is not propagating".into(),
                    );
                };
                let rates = self.z_rates.as_ref().map_or([None; 3], |r| r[c]);
                match super::hybrid_z::impedance_accuracy(&line_z(l1), &line_z(&l2), &rates) {
                    Some(a) => ZOutcome::Estimate(a),
                    None => ZOutcome::Unavailable("a refined line impedance is not finite".into()),
                }
            })
            .collect();
        (beta_acc, z)
    }
}

/// Face resolution at which a pair with `β²` (seen at `h`) resolves into two
/// real modes, from the matched pair on the `h/2` face.
///
/// Model (Krein collision): two real eigenvalues with continuum gap `g`
/// coupled at `O(h)` by the discretization (`c = Ch`) collide when
/// `c > g/2`, with `|Im β²| = √(C²h² − g²/4)`. So `|Im β²|²` is affine in
/// `h²`; the two levels fix `C²` and `g²/4` and the pair splits at
/// `h₀ = √(g²/4 / C²)`. Measured on the ε = 4, k₀ = 1.96 slab guide: the
/// b/8 + b/16 levels predict `h₀ = 0.032` and the fitted `g²/4 = 0.0099`
/// matches the oracle LSE₁₁/LSM₁₁ gap (`0.0102`); the pair actually splits
/// between h = 0.039 and 0.035. When the fit is not predictive
/// (`g²/4 ≤ 0`: still in the pre-asymptotic regime) the zero of the linear
/// `|Im β²|(h)` is used (conservative). `h/2` when the refined face already
/// resolves the pair (no pair nearby).
///
/// "Nearby" is `|Δ Re β²| ≤ ½ |β²|`, relative to the pair's own magnitude
/// (`|β²| > 0` for a complex pair), so the match is the same in every mesh
/// length unit and at every frequency (issue #828). It was
/// `½ max(|Re β²|, 1)`: an absolute `½` whenever `|Re β²| < 1`, which on a
/// µm-unit face at GHz (`β² ~ 1e-8 µm⁻²`) matched any pair.
fn pair_hint(beta_sq: c64, set_h2: &HybridPortModeSet, h: f64) -> Option<f64> {
    let im_h = beta_sq.im.abs();
    let near = set_h2
        .complex_pairs
        .iter()
        .min_by(|a, b| {
            (a.mode.beta_sq.re - beta_sq.re)
                .abs()
                .total_cmp(&(b.mode.beta_sq.re - beta_sq.re).abs())
        })
        .filter(|p| (p.mode.beta_sq.re - beta_sq.re).abs() <= 0.5 * beta_sq.norm());
    let h2 = 0.5 * h;
    match near {
        None => Some(h2),
        Some(p) => {
            let im_h2 = p.mode.beta_sq.im.abs();
            if im_h <= im_h2 {
                return None;
            }
            let c2 = (im_h * im_h - im_h2 * im_h2) / (h * h - h2 * h2);
            let gap = c2 * h2 * h2 - im_h2 * im_h2;
            if gap > 0.0 {
                Some((gap / c2).sqrt())
            } else {
                Some(h2 - im_h2 * (h - h2) / (im_h - im_h2))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::microstrip::{ShieldedStripFace, StripMeshOpts};
    use crate::driven::ports::{DEFAULT_MIN_TRACK_OVERLAP, HybridWavePortOpts, strip_line_section};

    /// A refined-face mode set holding only the complex pairs `pairs`.
    fn pair_set(pairs: &[c64]) -> HybridPortModeSet {
        HybridPortModeSet {
            modes: Vec::new(),
            n_propagating: 0,
            complex_pairs: pairs
                .iter()
                .map(|&beta_sq| HybridComplexPair {
                    mode: HybridComplexMode {
                        beta_sq,
                        beta: c64::new(0.0, 0.0),
                        e_t: Vec::new(),
                        e_z: Vec::new(),
                    },
                    conditioning: 1.0,
                    degenerate: false,
                    residual: 0.0,
                    real_evanescent_before: 0,
                })
                .collect(),
            diagnostics: Default::default(),
        }
    }

    /// Issue #828 regression: `pair_hint` is covariant in the mesh length
    /// unit `L` (`β² ∝ L⁻²`, `h ∝ L`, hint `∝ L`), at `L = 1`, `1e-6` and
    /// `1e3`. Case 1: the `h/2` face carries an unrelated pair 160 % away in
    /// `Re β²`, so the pair has resolved and the hint is `h/2`; the old
    /// absolute `½` window matched that unrelated pair once `|β²| ≪ 1`
    /// (`L = 1e3`). Case 2: the same pair at `h/2` with a smaller `|Im β²|`
    /// gives the Krein extrapolation.
    #[test]
    fn pair_hint_is_mesh_unit_covariant() {
        let beta_sq = c64::new(-3.0, -0.2);
        let unrelated = c64::new(-7.8, -0.1);
        let refined = c64::new(-3.05, -0.05);
        let h = 0.1;
        let hint = |l: f64, set: &[c64]| {
            let inv_l2 = 1.0 / (l * l);
            let scaled: Vec<c64> = set.iter().map(|z| z * inv_l2).collect();
            pair_hint(beta_sq * inv_l2, &pair_set(&scaled), h * l).map(|x| x / l)
        };
        for set in [&[unrelated][..], &[unrelated, refined][..]] {
            let reference = hint(1.0, set).expect("a finite hint");
            for l in [1e-6, 1e3] {
                let got = hint(l, set).expect("a finite hint");
                assert!(
                    (got - reference).abs() <= 1e-12 * reference,
                    "L = {l:e}: hint/L = {got} vs {reference} (set {set:?})"
                );
            }
        }
        assert_eq!(hint(1.0, &[unrelated]), Some(0.5 * h));
        assert!(hint(1.0, &[unrelated, refined]).unwrap() < 0.5 * h);
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn procrustes_and_sym_eig_are_exact_on_small_blocks() {
        let th = 0.3_f64;
        let o = vec![vec![th.cos(), th.sin()], vec![-th.sin(), th.cos()]];
        let (r, cos) = procrustes(&o, 2);
        // o R = (o oᵀ)^{1/2} = I for an orthogonal o.
        for i in 0..2 {
            for k in 0..2 {
                let v: f64 = (0..2).map(|j| o[i][j] * r[j][k]).sum();
                assert!((v - if i == k { 1.0 } else { 0.0 }).abs() < 1e-14);
            }
        }
        assert!(cos.iter().all(|c| (c - 1.0).abs() < 1e-14));
        // One channel onto a 3-cluster: R completes an orthonormal basis.
        let (r, cos) = procrustes(&[vec![0.6, 0.0, 0.8]], 3);
        assert!((cos[0] - 1.0).abs() < 1e-14);
        for a in 0..3 {
            for b in 0..3 {
                let v: f64 = (0..3).map(|j| r[j][a] * r[j][b]).sum();
                assert!((v - if a == b { 1.0 } else { 0.0 }).abs() < 1e-14);
            }
        }
        let (lam, _) = sym_eig(&[vec![2.0, 1.0], vec![1.0, 2.0]]);
        assert!((lam[0] - 3.0).abs() < 1e-14 && (lam[1] - 1.0).abs() < 1e-14);
    }

    /// The solver's basis inside an exactly degenerate cluster is arbitrary:
    /// emulate a different one at the second frequency by rotating the stored
    /// previous channels 45° inside the cluster. Per-vector overlaps are then
    /// ambiguous (measured 0.80 / 0.60, both above the 0.5 threshold, so the
    /// per-vector P2 tracker raised "mode identity lost"),
    /// while the subspace tracker captures the cluster (principal cosines 1)
    /// and its Procrustes rotation reproduces the rotated channels.
    #[test]
    fn subspace_tracking_survives_an_arbitrary_cluster_basis() {
        let eps = 2.2;
        let face = ShieldedStripFace {
            box_width: 8.0,
            box_height: 4.0,
            h: 2.0,
            strips: vec![[-1.75, -0.25], [0.25, 1.75]],
            thickness: 0.0,
            eps_below: eps,
            eps_above: eps,
        }
        .build(&StripMeshOpts {
            h_min: 0.15,
            h_max: 1.0,
            ratio: 1.6,
            mirror_symmetric: true,
        });
        let sec = strip_line_section(&face, 2, 1.0);
        let [f1, _] = sec.port_faces().unwrap();
        let port =
            HybridWavePort::new(f1, vec![c64::new(1.0, 0.0); 2]).with_opts(HybridWavePortOpts {
                accuracy: None,
                ..Default::default()
            });
        let edges = sec.extruded.mesh.edges();
        let ctx = FaceCtx::new(&sec.extruded.mesh, &edges, &port, 0).unwrap();
        let mut st = HybridState::new(ctx, 2, 2);
        st.channels_at(&port, 0, 0.1, 0, true).unwrap();
        assert_eq!(st.prev.len(), 2);
        // Rotate the stored channels by 45° inside the cluster.
        let (a, b) = (st.prev[0].clone(), st.prev[1].clone());
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let rot = [
            combine_modes(&[&a, &b], &[c, c]),
            combine_modes(&[&a, &b], &[-c, c]),
        ];
        st.prev = rot.to_vec();
        // Per-vector overlaps against the new solve are ambiguous.
        let set = st.ctx.solve(&port, 0.12, 2).unwrap().set;
        let me = sparse_matvec(st.ctx.m1.as_ref(), &st.prev[0].e_t);
        let ov: Vec<f64> = set
            .modes
            .iter()
            .take(2)
            .map(|m| {
                let w = transverse_pairing_from(&st.ctx.d, m);
                me.iter().zip(&w).map(|(x, y)| x * y).sum::<f64>().abs()
                    / (st.prev[0].norm * m.norm).abs().sqrt()
            })
            .collect();
        println!("per-vector overlaps of rotated channel 0: {ov:?}");
        assert!(ov.iter().all(|&o| o >= DEFAULT_MIN_TRACK_OVERLAP));
        // The subspace tracker succeeds and lands on the rotated basis.
        st.channels_at(&port, 0, 0.12, 1, false).unwrap();
        let pt = st.points[1].as_ref().unwrap();
        for (ch, want) in pt.channels.iter().zip(&rot) {
            let ovl = ch.track_overlap.unwrap();
            assert!(ovl >= 1.0 - 1e-9, "overlap {ovl}");
            let l = ch.line.as_ref().unwrap();
            let w = st.ctx.line(want, 0.1).unwrap();
            // Same direction: the current ratio is ω-independent for TEM.
            let (r, rw) = (l.currents[1] / l.currents[0], w.currents[1] / w.currents[0]);
            assert!((r - rw).abs() <= 1e-9 * rw.abs().max(1.0), "{r} vs {rw}");
        }
        assert!(st.cluster_warn.is_empty());
    }

    /// `ClusterSplit` on the real path. An *inhomogeneous* coupled
    /// microstrip (ε = 4.4 below, vacuum above) has distinct even/odd β², so
    /// no exact degeneracy exists. Emulate a tracked cluster that splits: at
    /// the first frequency store the 45° combinations of the even/odd pair
    /// as the previous channels and give them one β² so `channel_groups`
    /// sees a single degenerate cluster. At the next frequency the solve
    /// returns two simple modes; the cluster's subspace is still captured
    /// (principal cosines above the threshold) but spread over two units, so
    /// the tracker assigns inside it and records the split.
    #[test]
    fn cluster_split_fires_when_a_tracked_cluster_becomes_two_simple_modes() {
        let face = ShieldedStripFace {
            box_width: 8.0,
            box_height: 4.0,
            h: 2.0,
            strips: vec![[-1.75, -0.25], [0.25, 1.75]],
            thickness: 0.0,
            eps_below: 4.4,
            eps_above: 1.0,
        }
        .build(&StripMeshOpts {
            h_min: 0.15,
            h_max: 1.0,
            ratio: 1.6,
            mirror_symmetric: true,
        });
        let sec = strip_line_section(&face, 2, 1.0);
        let [f1, _] = sec.port_faces().unwrap();
        let port =
            HybridWavePort::new(f1, vec![c64::new(1.0, 0.0); 2]).with_opts(HybridWavePortOpts {
                accuracy: None,
                ..Default::default()
            });
        let edges = sec.extruded.mesh.edges();
        let ctx = FaceCtx::new(&sec.extruded.mesh, &edges, &port, 0).unwrap();
        let mut st = HybridState::new(ctx, 2, 2);
        st.channels_at(&port, 0, 0.1, 0, true).unwrap();
        assert_eq!(st.prev.len(), 2);
        // The first solve sees two simple modes (no cluster, no warning).
        assert!(
            (st.prev[0].beta_sq - st.prev[1].beta_sq).abs() > DEGENERATE_REL_TOL * st.prev_scale
        );
        assert!(st.cluster_warn.is_empty());
        // Replace the stored channels by a fake degenerate cluster: the 45°
        // combinations of the even/odd pair, sharing one β².
        let (a, b) = (st.prev[0].clone(), st.prev[1].clone());
        let c = std::f64::consts::FRAC_1_SQRT_2;
        let mut rot = [
            combine_modes(&[&a, &b], &[c, c]),
            combine_modes(&[&a, &b], &[-c, c]),
        ];
        let shared = a.beta_sq;
        for m in &mut rot {
            m.beta_sq = shared;
        }
        st.prev = rot.to_vec();
        assert_eq!(channel_groups(&st.prev, st.prev_scale), vec![vec![0, 1]]);
        // The split is tracked (no error) and warned about.
        st.channels_at(&port, 0, 0.12, 1, false).unwrap();
        println!("cluster_warn: {:?}", st.cluster_warn);
        assert_eq!(st.cluster_warn.len(), 1);
        match &st.cluster_warn[0] {
            PortWarningKind::ClusterSplit {
                channels,
                omega,
                min_cosine,
            } => {
                assert_eq!(channels, &vec![0, 1]);
                assert_eq!(*omega, 0.12);
                assert!(
                    *min_cosine >= DEFAULT_MIN_TRACK_OVERLAP && *min_cosine < 1.0 - 1e-6,
                    "min cosine {min_cosine}"
                );
            }
            w => panic!("expected ClusterSplit, got {w:?}"),
        }
        // Both channels are still assigned, with overlaps above the threshold.
        let pt = st.points[1].as_ref().unwrap();
        assert_eq!(pt.channels.len(), 2);
        for ch in &pt.channels {
            let ovl = ch.track_overlap.unwrap();
            println!("split-channel overlap {ovl:.4}");
            assert!(
                (DEFAULT_MIN_TRACK_OVERLAP..1.0 - 1e-6).contains(&ovl),
                "overlap {ovl}"
            );
        }
    }

    /// The #815 review's first follow-up note (Phase 5, #807): a failed
    /// refined (`h/2`) accuracy solve degrades to
    /// [`PortWarningKind::AccuracyUnavailable`] instead of aborting the sweep.
    /// The refinement is replaced by one whose every DOF is PEC, so its solve
    /// cannot succeed; the channel is still solved, tracked and reported.
    #[test]
    fn failed_refined_accuracy_solve_degrades_to_unavailable() {
        let face = ShieldedStripFace::microstrip(8.0, 5.0, 1.0, 1.0, 4.4).build(&StripMeshOpts {
            h_min: 0.15,
            h_max: 1.0,
            ratio: 1.6,
            mirror_symmetric: true,
        });
        let sec = strip_line_section(&face, 2, 1.0);
        let [f1, _] = sec.port_faces().unwrap();
        let port = HybridWavePort::new(f1, vec![c64::new(1.0, 0.0)]);
        assert!(port.opts.accuracy.is_some(), "accuracy is on by default");
        let edges = sec.extruded.mesh.edges();
        let solve_once = |break_refinement: bool| {
            let ctx = FaceCtx::new(&sec.extruded.mesh, &edges, &port, 0).unwrap();
            let mut st = HybridState::new(ctx, 1, 1);
            if break_refinement {
                let mut r = port.face.refine();
                r.interior_edge_mask.iter_mut().for_each(|m| *m = false);
                r.free_node_mask.iter_mut().for_each(|m| *m = false);
                assert!(solve_refined(&r, 0.1, &HybridPortOpts::default()).is_err());
                st.refined = Some(r);
            }
            st.channels_at(&port, 0, 0.1, 0, true)
                .expect("the sweep survives a failed accuracy solve");
            st.into_report(0)
        };
        // Healthy refinement: an estimate (β and Z), no unavailable warning.
        let (report, warnings) = solve_once(false);
        assert!(report.points[0].channels[0].accuracy.is_some());
        assert!(report.points[0].channels[0].impedance_accuracy.is_some());
        assert!(!warnings.iter().any(|w| matches!(
            w.kind,
            PortWarningKind::AccuracyUnavailable { .. }
                | PortWarningKind::ImpedanceAccuracyUnavailable { .. }
        )));
        // Broken refinement: no estimate, an `AccuracyUnavailable` warning,
        // and the same mode.
        let (broken, warnings) = solve_once(true);
        let ch = &broken.points[0].channels[0];
        assert!(ch.accuracy.is_none());
        assert_eq!(ch.beta, report.points[0].channels[0].beta);
        let unavailable: Vec<_> = warnings
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    PortWarningKind::AccuracyUnavailable {
                        channel: 0,
                        omega: 0.1
                    }
                )
            })
            .collect();
        assert_eq!(unavailable.len(), 1, "{warnings:?}");
        assert!(
            unavailable[0].message.contains("refined solve failed"),
            "{}",
            unavailable[0].message
        );
        // The line impedance degrades the same way (#807 review): no Z
        // estimate, an `ImpedanceAccuracyUnavailable` warning, the same Z.
        assert!(ch.impedance_accuracy.is_none());
        assert_eq!(
            ch.line.as_ref().unwrap().z_pi,
            report.points[0].channels[0].line.as_ref().unwrap().z_pi
        );
        let z_unavailable: Vec<_> = warnings
            .iter()
            .filter(|w| {
                matches!(
                    &w.kind,
                    PortWarningKind::ImpedanceAccuracyUnavailable { channel: 0, reason, .. }
                        if reason.contains("refined (h/2) solve failed")
                )
            })
            .collect();
        assert_eq!(z_unavailable.len(), 1, "{warnings:?}");
        assert!(
            z_unavailable[0]
                .message
                .contains("without an error estimate")
        );
    }
}
