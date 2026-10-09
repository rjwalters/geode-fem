//! The TM cutoff of a p=2 wave port's guide, **computed from the 3-D model**
//! (issue #955).
//!
//! The TE-only wave port needs a guard below the lowest TM cutoff of the
//! discrete guide behind it. The interim guard ([`tm_guard_margin`](super::tm_guard_margin), issue
//! #905) takes the face estimate less `max(5 %, 0.025·(k_c·h_n)²)`, a law
//! sized for the worst mesh at a given `k_c·h_n`, because the port face
//! cannot tell which mesh it has. On most coarse p=2 guides it is far wider
//! than the mesh needs: on a `2 × 1` guide with one tet layer of `h_n = b`
//! it gives up the top 45 % of the single-mode band, where the measured
//! 3-D cutoff is above the band.
//!
//! [`PortFaceProjection::guide_tm_guard`] measures the cutoff instead
//! (option 1 of the issue): it takes the tets of the guide footprint
//! within a depth of the port plane, closes every boundary of that section
//! with PEC, solves the p=2 cavity problem on it and takes the lowest mode
//! that carries axial field ([`TM_LIKE_AXIAL_SHARE`]). On a PEC box this is
//! the quantity `tm_guard_p2_measurement_table` (`tests/wave_port_p2.rs`)
//! measures. PEC is the closure that leaves the cutoff alone: the `β = 0`
//! TM field is axial, so it has no tangential component on a cap normal to
//! the guide.
//!
//! # Depth sensitivity and the claimed margin
//!
//! An unstructured section is cut along a jagged surface, and a closed box
//! has resonances of its own depth: on the Gmsh `3 × 1 × 4.06` box of issue
//! #905 two modes carry `E_z` near 2.90 where the continuum has none below
//! 3.31, and the same mesher at `d` = 4.00 or 4.10 has none. So the section
//! is solved at two depths, [`PortFaceProjection::guide_tm_guard`]'s `reach`
//! and [`TM_GUIDE_SHALLOW_FRACTION`] of it, and the computed value is used
//! only when the two agree to within [`TM_GUARD_COMPUTED_MARGIN`]. The
//! guard is then `(1 − δ_c)·min(k_deep, k_shallow, k_face)`, with
//! `δ_c` = [`TM_GUARD_COMPUTED_MARGIN`] and `k_face` the face estimate
//! ([`TmCutoffEstimate::k_c`]): never above the face, whose own modal
//! problem carries the TM mode from `k_face` on.
//!
//! # Fallback: the interim law, typed
//!
//! Wherever the computation does not apply, the guard is the interim
//! [`TmCutoffEstimate::guard_k_c`], and [`GuideTmGuard::source`] says why
//! ([`TmMarginLawReason`]): a p=1 solve (p=1 is untouched), an open rim
//! (the lateral PEC closure would be wrong), an empty section, no TM-like
//! mode resolved, a failed eigensolve, or the two depths disagreeing. In the
//! last case the guard is also clipped to `(1 − δ_c)·min(k_deep, k_shallow)`
//! when that is lower: a computed value is evidence even when it is not
//! robust.
//!
//! # What this does not do
//!
//! - **Enforce anything.** It returns a value and a note, like the interim
//!   guard. Under the operator's rule (never block a reasonable mesh) a
//!   caller that warns on it should warn, not reject.
//! - **Come for free.** It is a 3-D p=2 eigensolve of a section up to three
//!   TM-cutoff wavelengths deep, twice, per port: once per port per run,
//!   outside the frequency loop, and opt-in (nothing in the crate calls it).
//! - **See material.** The section is solved empty (geometric `k`, like the
//!   face estimate); scale by `1/√(Re ε_n·μ_t)` for a filled guide.

use burn::tensor::backend::Backend;
use faer::c64;

use super::wave_face::{PortFaceProjection, TmCutoffEstimate};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::eigen::pec_cavity::{
    PecCavityError, PecCavityMaterials, PecCavitySettings, solve_pec_cavity_modes_on_space,
};
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

/// Relative margin `δ_c` of a guard computed from the 3-D model (issue
/// #955), and the tolerance within which the two section depths must agree
/// for it to be used.
///
/// It is not derived. It absorbs what the computation leaves: the
/// eigensolve tolerance (`10⁻⁹`), the `Re ε_n` approximation of a lossy
/// fill (0.2 % at `tan δ_n = 0.067`, as in [`super::TM_GUARD_MARGIN`]), and
/// the cut, whose effect on the measured value is what the two-depth test
/// bounds. Smaller than the interim base margin because no mesh-dependent
/// undershoot is left to absorb: the computed value is the 3-D model's
/// cutoff, not the face's.
pub const TM_GUARD_COMPUTED_MARGIN: f64 = 0.02;

/// The shallower of the two section depths, as a fraction of the deeper
/// one ([`PortFaceProjection::guide_tm_guard`]): two TM-cutoff wavelengths
/// against three with the reach of [`super::tm_guard_axial_reach`].
pub const TM_GUIDE_SHALLOW_FRACTION: f64 = 2.0 / 3.0;

/// `|E·n|²` share (over the section, `n` the port normal) at and above which
/// a section mode counts as TM-like: the classifier of
/// `tm_guard_p2_measurement_table`. Below one half on purpose: at a box
/// degeneracy (TE₁₀ₚ against TM₁₁₀) the discrete model mixes the pair and
/// each branch carries about half the axial field; the lower branch is the
/// one a guard must stay below.
pub const TM_LIKE_AXIAL_SHARE: f64 = 0.4;

/// Where the TM cutoff of a [`GuideTmGuard`] comes from (issue #955).
#[derive(Debug, Clone, PartialEq)]
pub enum TmCutoffSource {
    /// Computed from the 3-D model: the lowest TM-like mode of the
    /// PEC-closed guide section at two depths, which agree to within
    /// [`TM_GUARD_COMPUTED_MARGIN`].
    Computed3d {
        /// `min(k_deep, k_shallow)` (geometric, rad / mesh unit).
        k_c: f64,
        /// The deeper section's depth (mesh units, either side of the
        /// port plane).
        depth: f64,
        /// Lowest TM-like mode of the deeper section.
        k_deep: f64,
        /// The shallower section's depth.
        depth_shallow: f64,
        /// Lowest TM-like mode of the shallower section (`k_deep` when both
        /// depths select the same tets and one solve served both).
        k_shallow: f64,
    },
    /// The interim margin law ([`tm_guard_margin`](super::tm_guard_margin)), because the
    /// computation does not apply.
    MarginLaw {
        /// Why.
        reason: TmMarginLawReason,
    },
}

/// Why a [`GuideTmGuard`] fell back to the interim margin law.
#[derive(Debug, Clone, PartialEq)]
pub enum TmMarginLawReason {
    /// The estimate guards a p=1 solve: p=1 keeps its law, bit for bit.
    ElementOrderP1,
    /// Part of the face rim is open (not on a conductor wall), so closing
    /// the section's lateral boundary with PEC would model another guide.
    OpenRim,
    /// No tet of the mesh lies over the port face.
    EmptySection,
    /// The cavity eigensolve of the section failed (the message).
    Eigensolve(String),
    /// No mode of the section carries [`TM_LIKE_AXIAL_SHARE`] of axial field
    /// among those resolved.
    NoTmLikeMode,
    /// The two section depths disagree by more than
    /// [`TM_GUARD_COMPUTED_MARGIN`]: the cut, or a box resonance of one
    /// depth, moves the value, so it is not the guide's. The guard is the
    /// margin law, clipped to the computed values when they are lower.
    DepthSensitive {
        /// The deeper section's depth.
        depth: f64,
        /// Its lowest TM-like mode.
        k_deep: f64,
        /// The shallower section's depth.
        depth_shallow: f64,
        /// Its lowest TM-like mode (`∞` if it has none).
        k_shallow: f64,
    },
}

/// The TM guard of a p=2 wave port with its cutoff computed from the 3-D
/// model where it can be ([`PortFaceProjection::guide_tm_guard`], issue
/// #955).
#[derive(Debug, Clone, PartialEq)]
pub struct GuideTmGuard {
    /// Where the cutoff comes from.
    pub source: TmCutoffSource,
    /// The geometric TM cutoff the guard rejects at (rad / mesh unit; scale
    /// by `1/√(Re ε_n·μ_t)` for a filled guide).
    pub guard_k_c: f64,
    /// The interim guard on the same estimate
    /// ([`TmCutoffEstimate::guard_k_c`]), for comparison.
    pub margin_law_guard_k_c: f64,
}

impl GuideTmGuard {
    /// `true` when the guard comes from the 3-D model.
    pub fn is_computed(&self) -> bool {
        matches!(self.source, TmCutoffSource::Computed3d { .. })
    }

    /// The guard's relative margin below the face estimate `k_face`:
    /// `1 − guard_k_c / k_face`.
    pub fn margin_below(&self, k_face: f64) -> f64 {
        1.0 - self.guard_k_c / k_face
    }

    /// A note for a caller that reports the guard: which source it came
    /// from and, on a fallback, why and what would restore the computed
    /// value. `None` for a computed guard. A note, never a rejection: under
    /// the margin law a coarse but usable mesh loses band, it is not
    /// blocked.
    pub fn note(&self) -> Option<String> {
        let TmCutoffSource::MarginLaw { reason } = &self.source else {
            return None;
        };
        let why = match reason {
            TmMarginLawReason::ElementOrderP1 => return None,
            TmMarginLawReason::OpenRim => {
                "the port rim is not all on a conductor wall, so the guide section cannot be \
                 closed with PEC"
                    .to_string()
            }
            TmMarginLawReason::EmptySection => "no tet lies over the port face".to_string(),
            TmMarginLawReason::Eigensolve(e) => {
                format!("the eigensolve of the guide section failed ({e})")
            }
            TmMarginLawReason::NoTmLikeMode => {
                "no mode of the guide section carries axial field among those resolved".to_string()
            }
            TmMarginLawReason::DepthSensitive {
                depth,
                k_deep,
                depth_shallow,
                k_shallow,
            } => format!(
                "the guide section's TM cutoff moves with the cut ({k_deep:.4} at depth \
                 {depth:.3}, {k_shallow:.4} at {depth_shallow:.3}: more than {:.0} % apart), so \
                 it is not the guide's; a uniform axial mesh near the port makes it robust",
                100.0 * TM_GUARD_COMPUTED_MARGIN
            ),
        };
        Some(format!(
            "the TM guard of this p=2 port is the interim margin law (guard k_c {:.4}): {why}",
            self.guard_k_c
        ))
    }
}

impl PortFaceProjection {
    /// The TM guard of a **p=2** wave port with the guide's TM cutoff
    /// computed from the 3-D model (issue #955; see the `wave_tm_guide`
    /// module docs for the method, the margin and the fallbacks).
    ///
    /// `estimate` is the face estimate with its axial spacing set
    /// ([`PortFaceProjection::tm_cutoff_estimate_at_order`] at p=2 and
    /// [`TmCutoffEstimate::with_axial_spacing`]); it supplies the interim
    /// guard, the cap `k_face` and the shift of the section eigensolve.
    /// `open_rim` is as in [`PortFaceProjection::lowest_tm_cutoff`]. `reach`
    /// is the deeper section's depth either side of the port plane, as for
    /// [`PortFaceProjection::guide_axial_spacing`]: use
    /// [`super::tm_guard_axial_reach`]. A depth beyond the guide's own
    /// extent over the face is clipped to it.
    ///
    /// For a p=1 estimate this returns the interim guard unchanged
    /// ([`TmMarginLawReason::ElementOrderP1`]) without touching the mesh.
    ///
    /// # Panics
    ///
    /// Panics if `open_rim` is given with a length other than
    /// `self.edges.len()`.
    pub fn guide_tm_guard<B: Backend>(
        &self,
        mesh: &TetMesh,
        estimate: &TmCutoffEstimate,
        open_rim: Option<&[bool]>,
        reach: f64,
        device: &B::Device,
    ) -> GuideTmGuard {
        let interim = estimate.guard_k_c();
        let fallback = |reason| GuideTmGuard {
            source: TmCutoffSource::MarginLaw { reason },
            guard_k_c: interim,
            margin_law_guard_k_c: interim,
        };
        if estimate.element_order != ElementOrder::P2 {
            return fallback(TmMarginLawReason::ElementOrderP1);
        }
        if let Some(o) = open_rim {
            assert_eq!(
                o.len(),
                self.edges.len(),
                "open_rim must be aligned with the port-face edges"
            );
            if o.iter()
                .zip(&self.interior_edge_mask)
                .any(|(&open, &interior)| open && !interior)
            {
                return fallback(TmMarginLawReason::OpenRim);
            }
        }
        let k_face = estimate.k_c();
        if !(k_face.is_finite() && k_face > 0.0) {
            // No TM mode on the face (or no estimate): nothing to compute.
            return fallback(TmMarginLawReason::NoTmLikeMode);
        }
        let footprint = self.guide_footprint(mesh);
        if footprint.is_empty() {
            return fallback(TmMarginLawReason::EmptySection);
        }
        let span = footprint.iter().fold(0.0_f64, |m, &(_, _, d)| m.max(d));
        let depth = reach.max(0.0).min(span);
        let depth_shallow = TM_GUIDE_SHALLOW_FRACTION * depth;
        let select = |dmax: f64| -> Vec<usize> {
            footprint
                .iter()
                .filter(|&&(_, _, d)| d <= dmax)
                .map(|&(t, _, _)| t)
                .collect()
        };
        let (deep, shallow) = (select(depth), select(depth_shallow));
        let solve = |tets: &[usize]| {
            guide_section_tm_cutoff::<B>(&section_mesh(mesh, tets), self.normal, k_face, device)
        };
        let k_deep = match solve(&deep) {
            Ok(Some(k)) => k,
            Ok(None) => return fallback(TmMarginLawReason::NoTmLikeMode),
            Err(e) => return fallback(TmMarginLawReason::Eigensolve(e.to_string())),
        };
        let k_shallow = if shallow == deep {
            k_deep
        } else {
            match solve(&shallow) {
                Ok(Some(k)) => k,
                Ok(None) => f64::INFINITY,
                Err(e) => return fallback(TmMarginLawReason::Eigensolve(e.to_string())),
            }
        };
        let k_c = k_deep.min(k_shallow);
        let computed = (1.0 - TM_GUARD_COMPUTED_MARGIN) * k_c.min(k_face);
        if (k_deep - k_shallow).abs() > TM_GUARD_COMPUTED_MARGIN * k_deep.max(k_shallow)
            || !k_shallow.is_finite()
        {
            return GuideTmGuard {
                source: TmCutoffSource::MarginLaw {
                    reason: TmMarginLawReason::DepthSensitive {
                        depth,
                        k_deep,
                        depth_shallow,
                        k_shallow,
                    },
                },
                guard_k_c: interim.min(computed),
                margin_law_guard_k_c: interim,
            };
        }
        GuideTmGuard {
            source: TmCutoffSource::Computed3d {
                k_c,
                depth,
                k_deep,
                depth_shallow,
                k_shallow,
            },
            guard_k_c: computed,
            margin_law_guard_k_c: interim,
        }
    }
}

/// The sub-mesh of `mesh` made of the tets `tets` (indices into
/// `mesh.tets`, in that order), with its nodes renumbered in ascending
/// original order. When `tets` is every tet, the result is `mesh` itself
/// (without its physical groups).
fn section_mesh(mesh: &TetMesh, tets: &[usize]) -> TetMesh {
    let mut used = vec![false; mesh.nodes.len()];
    for &t in tets {
        for &n in &mesh.tets[t] {
            used[n as usize] = true;
        }
    }
    let mut map = vec![u32::MAX; mesh.nodes.len()];
    let mut nodes = Vec::new();
    for (i, &u) in used.iter().enumerate() {
        if u {
            map[i] = nodes.len() as u32;
            nodes.push(mesh.nodes[i]);
        }
    }
    TetMesh {
        nodes,
        tets: tets
            .iter()
            .map(|&t| mesh.tets[t].map(|n| map[n as usize]))
            .collect(),
        physical_groups: Default::default(),
    }
}

/// Largest number of section modes [`guide_section_tm_cutoff`] asks for
/// before it gives up on finding a TM-like one.
const MAX_SECTION_MODES: usize = 96;

/// The lowest TM-like resonance of `section` closed with PEC on its whole
/// boundary, at p=2 (issue #955): of the modes nearest `(0.45·k_ref)²`, the
/// lowest whose `|E·normal|²` share, sampled at five points of every tet, is
/// at least [`TM_LIKE_AXIAL_SHARE`]. `k_ref` is the face estimate of the TM
/// cutoff. The first solve asks for 24 modes (the settings of
/// `tm_guard_p2_measurement_table`), and the request doubles up to
/// 96 (`MAX_SECTION_MODES`) while no TM-like mode is found and the modes still
/// end below `k_ref`. `Ok(None)` when none is found.
///
/// # Errors
///
/// The eigensolve's, and a section whose PEC mask cannot be built.
pub fn guide_section_tm_cutoff<B: Backend>(
    section: &TetMesh,
    normal: [f64; 3],
    k_ref: f64,
    device: &B::Device,
) -> Result<Option<f64>, PecCavityError> {
    let space = HcurlSpace::build(section, ElementOrder::P2);
    let walls = section.boundary_faces();
    let mask = space
        .pec_interior_mask(section, &[&walls])
        .map_err(|e| PecCavityError::InvalidInput(e.to_string()))?;
    let eps = vec![1.0; section.n_tets()];
    let kept: Vec<usize> = (0..space.n_dofs()).filter(|&d| mask[d]).collect();
    let pts = [
        [0.25, 0.25, 0.25, 0.25],
        [0.55, 0.15, 0.15, 0.15],
        [0.15, 0.55, 0.15, 0.15],
        [0.15, 0.15, 0.55, 0.15],
        [0.15, 0.15, 0.15, 0.55],
    ];
    let mut n_modes = 24;
    loop {
        let mut settings = PecCavitySettings::new((0.45 * k_ref).powi(2), n_modes);
        settings.max_iters = 480.max(5 * n_modes);
        let modes = match solve_pec_cavity_modes_on_space::<B>(
            &space,
            section,
            &PecCavityMaterials::Isotropic(&eps),
            &mask,
            &[],
            &settings,
            device,
        ) {
            Ok(m) => m.modes.modes,
            Err(PecCavityError::TooFewModes { found, .. }) if found > 0 && found < n_modes => {
                n_modes = found;
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut shares: Vec<(f64, f64)> = modes
            .iter()
            .map(|m| {
                let mut x = vec![c64::new(0.0, 0.0); space.n_dofs()];
                for (i, &d) in kept.iter().enumerate() {
                    x[d] = c64::new(m.vector[i], 0.0);
                }
                let (mut axial, mut all) = (0.0, 0.0);
                for t in 0..section.n_tets() {
                    for bary in pts {
                        let e = space.field_at(section, t, bary, &x);
                        let en = e[0] * normal[0] + e[1] * normal[1] + e[2] * normal[2];
                        axial += en.norm_sqr();
                        all += e[0].norm_sqr() + e[1].norm_sqr() + e[2].norm_sqr();
                    }
                }
                (m.k0, axial / all)
            })
            .collect();
        shares.sort_by(|p, q| p.0.total_cmp(&q.0));
        if let Some(&(k, _)) = shares.iter().find(|m| m.1 >= TM_LIKE_AXIAL_SHARE) {
            return Ok(Some(k));
        }
        let top = shares.last().map_or(0.0, |m| m.0);
        if shares.len() < n_modes || top >= k_ref || 2 * n_modes > MAX_SECTION_MODES {
            return Ok(None);
        }
        n_modes *= 2;
    }
}
