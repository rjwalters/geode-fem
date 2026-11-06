//! `geode eigen` with `eigen.bloch`: unit-cell band solves on a periodic
//! mesh (Epic #837 Phase 4a).
//!
//! A thin layer over the core machinery of Phase 2: the matched
//! [`PeriodicMap`](geode_core::mesh::periodic::PeriodicMap) built at load
//! time ([`crate::problem::Problem::periodic`], so `geode check` and the
//! solve share one pairing) becomes a
//! [`PeriodicConstraint`],
//! which a [`BlochCell`] turns into the reduced Hermitian pencil at each
//! wave vector. No solver logic lives here.
//!
//! - `bloch.k` — one [`BlochCell::solve`]; the `n_modes` bands closest to
//!   the shift, ascending in frequency (a Γ-equivalent `k` returns its
//!   harmonic fields as `0 Hz` modes).
//! - `bloch.path` — [`BlochCell::band_structure`] along the labelled
//!   path: overlap-tracked bands with their group velocity along the path.
//!
//! The shift `eigen.shift` is the shift-invert target `σ = k₀²`; to get the
//! lowest bands pick it just below the first one. `eigen.max_iters` is a
//! cavity-Lanczos setting and is ignored here (the block solver keeps its
//! core default basis cap); `eigen.tol` is the block solver's residual
//! bound and `eigen.residual_tol` the acceptance gate on every reported
//! band, as for a cavity.

use burn::tensor::backend::BackendTypes;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::PeriodicConstraint;
use geode_core::constants::C_M_PER_S;
use geode_core::eigen::bloch::{BlochCell, BlochSettings, KPath};
use geode_core::eigen::pec_cavity::PecCavityMaterials;
use geode_core::elements::ElementOrder;

use crate::backend::CompiledBackend;
use crate::error::CliError;
use crate::problem::{BlochTarget, EigenTarget, Problem};
use crate::report::{BandRow, BlochBands};

type B = CompiledBackend;

/// Extra modes computed per k-point along a path so a band can leave the
/// reported window and still be tracked (core's `n_guard`).
const PATH_GUARD_BANDS: usize = 4;

/// The periodic DOF constraint of a resolved periodic problem: the matched
/// map over the H(curl) space, with the spec's PEC walls eliminated.
///
/// # Errors
///
/// `invalid_spec` if the problem has no periodic pairs or the constraint
/// does not match the mesh.
pub fn constraint(p: &Problem) -> Result<PeriodicConstraint, CliError> {
    let setup = p.periodic.as_ref().ok_or_else(|| {
        CliError::InvalidSpec("no `boundary_conditions.periodic` pairs".to_string())
    })?;
    let space = HcurlSpace::build(&p.tagged.mesh, ElementOrder::P1);
    Ok(PeriodicConstraint::build(
        &space,
        &setup.map,
        Some(&p.pec_mask),
    )?)
}

/// The solved band table plus the largest residual and the pencil size.
pub struct Solved {
    /// The report table.
    pub bands: BlochBands,
    /// Largest relative residual over every reported band.
    pub residual_rel_max: f64,
}

/// Solve the Bloch request of `p`.
pub fn solve(p: &Problem, target: &EigenTarget, bloch: &BlochTarget) -> Result<Solved, CliError> {
    let constraint = constraint(p)?;
    let device = <B as BackendTypes>::Device::default();
    let cell = if p.is_anisotropic() {
        let (eps, nu) = p.real_diagonal_materials();
        BlochCell::new::<B>(
            &p.tagged.mesh,
            &PecCavityMaterials::Diagonal { eps: &eps, nu: &nu },
            &constraint,
            &device,
        )?
    } else {
        let eps_r: Vec<f64> = p.eps.iter().map(|e| e.re).collect();
        BlochCell::new::<B>(
            &p.tagged.mesh,
            &PecCavityMaterials::Isotropic(&eps_r),
            &constraint,
            &device,
        )?
    };
    let settings = BlochSettings {
        sigma: Some(target.sigma()),
        tol: target.tol,
        ..BlochSettings::new(target.n_modes)
    };
    // k₀ [rad / mesh unit] → f [Hz].
    let k0_to_hz = C_M_PER_S / (2.0 * std::f64::consts::PI * p.length_unit_m());
    let report = constraint.report();

    let (mode, rows, mut warnings) = match bloch {
        BlochTarget::Point(k) => {
            let modes = cell.solve(*k, &settings)?;
            let row = BandRow {
                index: 0,
                k: *k,
                s: None,
                label: None,
                frequency_hz: modes.modes.iter().map(|m| m.omega * k0_to_hz).collect(),
                residual_rel: modes.modes.iter().map(|m| m.residual_rel).collect(),
                v_g_over_c: None,
            };
            ("point", vec![row], modes.warnings)
        }
        BlochTarget::Path {
            vertices,
            points_per_segment,
        } => {
            let path = KPath::new(vertices.clone(), *points_per_segment)?;
            let bands = cell.band_structure(&path, &settings, PATH_GUARD_BANDS)?;
            let rows = bands
                .points
                .iter()
                .enumerate()
                .map(|(i, pt)| BandRow {
                    index: i,
                    k: pt.k,
                    s: Some(pt.s),
                    label: pt.label.clone(),
                    frequency_hz: bands.bands.iter().map(|b| b.omega[i] * k0_to_hz).collect(),
                    residual_rel: bands.bands.iter().map(|b| b.residual_rel[i]).collect(),
                    v_g_over_c: Some(bands.bands.iter().map(|b| b.group_velocity[i]).collect()),
                })
                .collect();
            ("path", rows, bands.warnings)
        }
    };

    let mut residual_rel_max = 0.0f64;
    for row in &rows {
        for (band, (&f, &r)) in row.frequency_hz.iter().zip(&row.residual_rel).enumerate() {
            if !(f.is_finite() && r.is_finite()) {
                return Err(CliError::NonFinite {
                    index: band,
                    what: format!(
                        "Bloch band {band} at k = {:?}: f = {f}, residual = {r}",
                        row.k
                    ),
                });
            }
            if r > target.residual_tol {
                return Err(CliError::BlochSolve(format!(
                    "band {band} at k = {:?} has relative residual {r:e} > eigen.residual_tol \
                     {:e}: unconverged (tighten eigen.tol or raise eigen.residual_tol)",
                    row.k, target.residual_tol
                )));
            }
            residual_rel_max = residual_rel_max.max(r);
        }
    }
    if let Some(setup) = &p.periodic {
        warnings.extend(setup.map.report().warnings());
    }
    warnings.extend(constraint.warnings());
    Ok(Solved {
        bands: BlochBands {
            mode,
            n_bands: target.n_modes,
            n_full: report.n_full,
            n_reduced: report.n_reduced,
            rows,
            warnings,
        },
        residual_rel_max,
    })
}

/// Number of k-points a Bloch request solves.
pub fn n_k_points(bloch: &BlochTarget) -> usize {
    match bloch {
        BlochTarget::Point(_) => 1,
        // A segment's end vertex is the next segment's first sample.
        BlochTarget::Path {
            vertices,
            points_per_segment,
        } => (vertices.len() - 1) * points_per_segment + 1,
    }
}
