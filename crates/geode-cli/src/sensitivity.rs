//! Material design sensitivities through the CLI (issue #707, Epic #702
//! Phase 5): the spec's optional `sensitivity` section
//! ([`crate::spec::SensitivitySpec`], resolved to
//! [`crate::problem::SensitivityTarget`]) adds a `sensitivities` block
//! ([`SensitivityReport`]) to the capacitance, inductance and eigen reports.
//!
//! No new adjoint math lives here: each analysis wires the library's
//! existing FD-validated gradient (Epic #569) and only aggregates / scales
//! it.
//!
//! * **Capacitance** (one terminal) —
//!   [`geode_core::adjoint::capacitance_adjoint_gradient_p2`]: the
//!   two-terminal **P2** capacitance `C = 2W/V²` and `∂C/∂ε_k` from one
//!   forward + one adjoint solve. The P2 `C` is the observable (reported
//!   per entry) — the report's `c_farad` is the P1 extraction.
//! * **Inductance** —
//!   [`geode_core::adjoint::inductance_adjoint_sensitivity`]: the full
//!   `∂L_ij/∂ν_k` tensor from the self-adjoint energy form (no extra
//!   solve); `∂L/∂μ_r = −ν_r² ∂L/∂ν_r`.
//! * **Eigen** (lossless) —
//!   [`geode_core::eigen::sensitivity::EigenSensitivity::deigenvalue_deps`]
//!   (Hellmann–Feynman on the converged `M_ε`-normalized eigenpair, with
//!   the simple-eigenvalue gap guard), chained through
//!   [`geode_core::eigen::transmon::frequency_hz_from_lambda`]:
//!   `f = c√λ/(2πL)` ⇒ `∂f/∂ε = (f / 2λ) ∂λ/∂ε`.
//!
//! The library returns one gradient per **design region**; the CLI makes
//! every volume region a design region (label = index into
//! [`crate::problem::Problem::regions`]) and reports the regions the spec
//! names. Gradients are scaled to SI exactly like the observable
//! (`× mesh.length_unit_m` for `C` and `L`).
//!
//! The optional `fd_check` re-solves the **shipped forward pipeline** (not
//! the adjoint routine) at `p·(1 ± relative_step)` per parameter and
//! fails the run (`solve_failed`) when any entry disagrees beyond the
//! tolerance.

use std::time::Instant;

use geode_core::adjoint::{capacitance_adjoint_gradient_p2, inductance_adjoint_sensitivity};
use geode_core::assembly::electrostatic::{Electrode, assemble_electrostatic_p2};
use geode_core::assembly::magnetostatic3d::{
    CurrentTerminal, assemble_magnetostatic3d, extract_inductance,
};
use geode_core::eigen::sensitivity::EigenSensitivity;
use geode_core::eigen::transmon::frequency_hz_from_lambda;

use crate::error::CliError;
use crate::problem::{CapacitanceTarget, Problem, SensitivityTarget};
use crate::report::{
    FdCheckSummary, SensitivityEntry, SensitivityParameterSummary, SensitivityReport,
};
use crate::spec::SensitivityParameterKind;

/// Fraction of a component's natural gradient scale `|value / p|` (the
/// gradient of an observable proportional to the parameter) used as the
/// floor of the FD-check denominator.
pub const FD_SCALE_FLOOR: f64 = 1e-2;

/// FD-check disagreement `|a − b| / max(|a|, |b|, floor)` (`0` when the
/// denominator is exactly zero). `floor` keeps a structurally ~zero
/// component (e.g. a mutual inductance w.r.t. a region only one path's
/// field fills) from being judged against central-difference round-off.
pub fn rel_error(a: f64, b: f64, floor: f64) -> f64 {
    let scale = a.abs().max(b.abs()).max(floor);
    if scale == 0.0 {
        0.0
    } else {
        (a - b).abs() / scale
    }
}

/// The gradient of every observable component w.r.t. every parameter,
/// before the optional FD check.
struct Gradients {
    /// Per component: its index and its value (observable unit).
    components: Vec<(Vec<usize>, f64)>,
    /// `grad[parameter][component]`.
    grad: Vec<Vec<f64>>,
}

/// Per-tet values with every tet of design region `region` set to
/// `value`.
fn with_region_value(base: &[f64], region_of_tet: &[usize], region: usize, value: f64) -> Vec<f64> {
    base.iter()
        .zip(region_of_tet)
        .map(|(&b, &r)| if r == region { value } else { b })
        .collect()
}

/// How the FD check places its two evaluation points.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FdPoints {
    /// `p ± h` as given (the forward consumes `f64` parameters).
    Exact,
    /// `p ± h` snapped to the nearest `f32`, with the FD denominator the
    /// snapped spacing. The Nédélec mass assembly the eigen forward uses
    /// (`assemble_global_nedelec_with_complex_epsilon_sparse`) uploads the
    /// per-tet `ε_r` as `f32`, so a raw `p ± h` would be quantized to
    /// `~6e-8` relative — an FD error of `6e-8 / relative_step`
    /// (`≈ 1.7e-4` measured at the default step, above the `1e-4` bar).
    /// Evaluating at representable points and dividing by their true
    /// spacing removes that error exactly. Removable once issue #740
    /// (the f32 upload) is fixed.
    SnapToF32,
}

/// Wrap gradients into the report, running the FD self-check with
/// `forward(parameter, perturbed_value)` (component values in the
/// observable unit, same order as `g.components`) when requested.
#[allow(clippy::too_many_arguments)]
fn finish(
    sens: &SensitivityTarget,
    observable: &'static str,
    observable_unit: &'static str,
    method: &'static str,
    g: Gradients,
    describe: impl Fn(&[usize]) -> String,
    points: FdPoints,
    mut forward: impl FnMut(usize, f64) -> Result<Vec<f64>, CliError>,
    t0: Instant,
) -> Result<SensitivityReport, CliError> {
    let mut entries = Vec::with_capacity(sens.parameters.len() * g.components.len());
    for (k, row) in g.grad.iter().enumerate() {
        for ((index, value), &gradient) in g.components.iter().zip(row) {
            if !(gradient.is_finite() && value.is_finite()) {
                return Err(CliError::NonFinite {
                    index: k,
                    what: format!(
                        "sensitivity of {} w.r.t. parameter {k} is {gradient}",
                        describe(index)
                    ),
                });
            }
            entries.push(SensitivityEntry {
                parameter: k,
                index: index.clone(),
                value: *value,
                gradient,
                fd_gradient: None,
                fd_rel_error: None,
            });
        }
    }
    let fd_check = match sens.fd_check {
        None => None,
        Some((step, tol)) => {
            let n_comp = g.components.len();
            let mut max_rel = 0.0_f64;
            let mut worst: Option<String> = None;
            for (k, prm) in sens.parameters.iter().enumerate() {
                let h = step * prm.value.abs();
                let (mut p_plus, mut p_minus) = (prm.value + h, prm.value - h);
                if points == FdPoints::SnapToF32 {
                    (p_plus, p_minus) = (f64::from(p_plus as f32), f64::from(p_minus as f32));
                }
                let plus = forward(k, p_plus)?;
                let minus = forward(k, p_minus)?;
                for c in 0..n_comp {
                    let fd = (plus[c] - minus[c]) / (p_plus - p_minus);
                    let e = &mut entries[k * n_comp + c];
                    let floor = FD_SCALE_FLOOR * (e.value / prm.value).abs();
                    let rel = rel_error(e.gradient, fd, floor);
                    // A NaN disagreement ranks as the worst possible.
                    let key = if rel.is_nan() { f64::INFINITY } else { rel };
                    if worst.is_none() || key > max_rel {
                        max_rel = key;
                        worst = Some(format!(
                            "parameter `{}` ({}), {}: gradient {:.6e} vs central FD {fd:.6e} \
                             (rel {rel:.3e})",
                            prm.physical_group,
                            prm.kind.name(),
                            describe(&e.index),
                            e.gradient
                        ));
                    }
                    e.fd_gradient = Some(fd);
                    e.fd_rel_error = Some(rel);
                }
            }
            if max_rel > tol {
                return Err(CliError::FdCheckFailed {
                    detail: worst.unwrap_or_default(),
                    tolerance: tol,
                });
            }
            Some(FdCheckSummary {
                relative_step: step,
                tolerance: tol,
                max_rel_error: max_rel,
                n_forward_solves: 2 * sens.parameters.len(),
            })
        }
    };
    Ok(SensitivityReport {
        observable,
        observable_unit,
        method,
        parameters: sens
            .parameters
            .iter()
            .map(|p| SensitivityParameterSummary {
                physical_group: p.physical_group.clone(),
                kind: p.kind.name(),
                value: p.value,
            })
            .collect(),
        entries,
        fd_check,
        wall_time_s: t0.elapsed().as_secs_f64(),
    })
}

/// `∂C/∂ε_r` of the two-terminal (one terminal + ground) P2 capacitance.
pub fn capacitance(
    p: &Problem,
    target: &CapacitanceTarget,
    sens: &SensitivityTarget,
) -> Result<SensitivityReport, CliError> {
    let t0 = Instant::now();
    let mesh = &p.tagged.mesh;
    let scale = p.length_unit_m();
    // `problem::load` enforced exactly one terminal for a sensitivity spec.
    let electrodes = [Electrode {
        name: target.terminals[0].surface.name.clone(),
        nodes: target.terminals[0].nodes.clone(),
        voltage: 1.0,
    }];
    let eps_r: Vec<f64> = p.eps.iter().map(|e| e.re).collect();
    let res = capacitance_adjoint_gradient_p2(
        mesh,
        &eps_r,
        &electrodes,
        &target.ground_nodes,
        &sens.region_of_tet,
        sens.n_regions,
    )?;
    let g = Gradients {
        components: vec![(Vec::new(), res.capacitance * scale)],
        grad: sens
            .parameters
            .iter()
            .map(|prm| vec![res.grad[prm.region] * scale])
            .collect(),
    };
    // FD: the shipped P2 forward (assemble → solve → C = 2W/V²).
    let rho = vec![0.0_f64; mesh.n_tets()];
    let forward = |k: usize, value: f64| -> Result<Vec<f64>, CliError> {
        let er = with_region_value(
            &eps_r,
            &sens.region_of_tet,
            sens.parameters[k].region,
            value,
        );
        let sys = assemble_electrostatic_p2(mesh, &er, &rho, &electrodes, &target.ground_nodes)?;
        let u = sys.solve()?;
        Ok(vec![2.0 * sys.field_energy(&u) * scale])
    };
    finish(
        sens,
        "c_farad",
        "F",
        "adjoint_p2",
        g,
        |_| format!("C({})", electrodes[0].name),
        FdPoints::Exact,
        forward,
        t0,
    )
}

/// `∂L_ij/∂ν_r` (or `∂L_ij/∂μ_r`) of every inductance-matrix entry.
/// `terminals` are the solved paths' unit-current excitations, in matrix
/// order; `tol_solenoidal` is the forward solve's gate.
pub fn inductance(
    p: &Problem,
    terminals: &[CurrentTerminal],
    tol_solenoidal: f64,
    sens: &SensitivityTarget,
) -> Result<SensitivityReport, CliError> {
    let t0 = Instant::now();
    let mesh = &p.tagged.mesh;
    let scale = p.length_unit_m();
    let nu_r: Vec<f64> = p.mu_r.iter().map(|&m| 1.0 / m).collect();
    let res = inductance_adjoint_sensitivity(
        mesh,
        &nu_r,
        &p.pec_mask,
        terminals,
        &sens.region_of_tet,
        sens.n_regions,
        tol_solenoidal,
    )?;
    let n = terminals.len();
    let mut components = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            components.push((vec![i, j], res.l[i][j] * scale));
        }
    }
    let grad = sens
        .parameters
        .iter()
        .map(|prm| {
            // ∂L/∂μ = ∂L/∂ν · dν/dμ = −ν² ∂L/∂ν.
            let chain = match prm.kind {
                SensitivityParameterKind::MuR => -1.0 / (prm.value * prm.value),
                _ => 1.0,
            };
            let d = &res.dl_dnu[prm.region];
            (0..n * n)
                .map(|c| d[c / n][c % n] * chain * scale)
                .collect()
        })
        .collect();
    let g = Gradients { components, grad };
    // FD: the shipped forward (assemble_magnetostatic3d → extract_inductance).
    let forward = |k: usize, value: f64| -> Result<Vec<f64>, CliError> {
        let prm = &sens.parameters[k];
        let mu_value = match prm.kind {
            SensitivityParameterKind::MuR => value,
            _ => 1.0 / value,
        };
        let mu = with_region_value(&p.mu_r, &sens.region_of_tet, prm.region, mu_value);
        let sys = assemble_magnetostatic3d(mesh, &mu, &p.pec_mask)?;
        let l = extract_inductance(&sys, mesh, terminals, tol_solenoidal)?.l;
        Ok(l.iter().flatten().map(|v| v * scale).collect())
    };
    let names: Vec<&str> = terminals.iter().map(|t| t.name.as_str()).collect();
    finish(
        sens,
        "l_henry",
        "H",
        "self_adjoint_energy",
        g,
        |idx| format!("L[{}][{}]", names[idx[0]], names[idx[1]]),
        FdPoints::Exact,
        forward,
        t0,
    )
}

/// `∂f/∂ε_r` of the `sensitivity.modes` resonant frequencies of a
/// lossless cavity. `lambdas` / `vectors` are every returned mode's
/// eigenvalue and `M_ε`-normalized interior eigenvector (ascending);
/// `resolve(eps_r)` re-solves the same pencil for the FD check and
/// returns its eigenvalues.
pub fn eigen(
    p: &Problem,
    lambdas: &[f64],
    vectors: &[Vec<f64>],
    sens: &SensitivityTarget,
    mut resolve: impl FnMut(&[f64]) -> Result<Vec<f64>, CliError>,
) -> Result<SensitivityReport, CliError> {
    let t0 = Instant::now();
    let mesh = &p.tagged.mesh;
    let lu = p.length_unit_m();
    let eps_r: Vec<f64> = p.eps.iter().map(|e| e.re).collect();
    let mut components = Vec::with_capacity(sens.modes.len());
    // dλ/dε per differentiated mode, then chained to df/dε.
    let mut dfreq: Vec<Vec<f64>> = Vec::with_capacity(sens.modes.len());
    for &m in &sens.modes {
        let es = EigenSensitivity {
            mesh,
            edges: &p.edges,
            interior_mask: &p.pec_mask,
            eps_r: &eps_r,
            lambdas,
            mode_index: m,
            eigenvector: &vectors[m],
            min_rel_gap: sens.min_rel_gap,
        };
        let dl = es
            .deigenvalue_deps(&sens.region_of_tet, sens.n_regions)
            .map_err(CliError::EigenSensitivity)?;
        let lambda = lambdas[m];
        let f = frequency_hz_from_lambda(lambda, lu);
        // f = c√λ/(2πL) ⇒ df/dλ = f / (2λ).
        let df_dl = f / (2.0 * lambda);
        components.push((vec![m], f));
        dfreq.push(dl.iter().map(|d| d * df_dl).collect());
    }
    let grad = sens
        .parameters
        .iter()
        .map(|prm| dfreq.iter().map(|d| d[prm.region]).collect())
        .collect();
    let g = Gradients { components, grad };
    let base: Vec<f64> = sens.modes.iter().map(|&m| lambdas[m]).collect();
    // FD: re-solve the pencil and follow each mode to the nearest
    // perturbed eigenvalue.
    let forward = |k: usize, value: f64| -> Result<Vec<f64>, CliError> {
        let er = with_region_value(
            &eps_r,
            &sens.region_of_tet,
            sens.parameters[k].region,
            value,
        );
        let solved = resolve(&er)?;
        Ok(base
            .iter()
            .map(|&l0| {
                let nearest = solved
                    .iter()
                    .copied()
                    .min_by(|a, b| (a - l0).abs().total_cmp(&(b - l0).abs()))
                    .unwrap_or(f64::NAN);
                frequency_hz_from_lambda(nearest, lu)
            })
            .collect())
    };
    finish(
        sens,
        "frequency_hz",
        "Hz",
        "hellmann_feynman",
        g,
        |idx| format!("mode {} frequency", idx[0]),
        FdPoints::SnapToF32,
        forward,
        t0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_error_is_symmetric_and_zero_safe() {
        assert_eq!(rel_error(0.0, 0.0, 0.0), 0.0);
        assert_eq!(rel_error(1.0, 1.0, 0.0), 0.0);
        assert!((rel_error(1.0, 0.9, 0.0) - 0.1).abs() < 1e-15);
        assert!((rel_error(0.9, 1.0, 0.0) - 0.1).abs() < 1e-15);
        assert_eq!(rel_error(1.0, 0.0, 0.0), 1.0);
        assert!(rel_error(1.0, f64::NAN, 0.0).is_nan());
        // The floor only matters for components far below it.
        assert!((rel_error(1.0, 0.9, 1e-3) - 0.1).abs() < 1e-15);
        assert!((rel_error(1e-8, 2e-8, 1.0) - 1e-8).abs() < 1e-20);
    }

    #[test]
    fn region_value_replaces_only_that_region() {
        assert_eq!(
            with_region_value(&[1.0, 2.0, 3.0, 4.0], &[0, 1, 0, 2], 0, 9.0),
            vec![9.0, 2.0, 9.0, 4.0]
        );
    }
}
