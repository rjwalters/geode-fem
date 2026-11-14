//! EQS-spec validation and resolution (issue #1494).

use super::*;
use crate::spec::{EqsSpec, WaveformSpec};

/// Upper bound on `transient` steps (the history is held in memory).
const MAX_EQS_STEPS: usize = 1_000_000;
/// Relative tolerance for `t_end` being an integer multiple of `dt`.
const END_TIME_REL_TOL: f64 = 1e-9;

fn finite_nonneg(what: &str, x: f64) -> Result<(), CliError> {
    if x.is_finite() && x >= 0.0 {
        Ok(())
    } else {
        Err(invalid(format!("{what} must be finite and >= 0 (got {x})")))
    }
}

fn validate_waveform(terminal: &str, w: &WaveformSpec) -> Result<(), CliError> {
    let at = format!("eqs.terminals[{terminal}].waveform");
    match w {
        WaveformSpec::Step { t0, v } => {
            finite_nonneg(&format!("{at}.t0"), *t0)?;
            if !v.is_finite() {
                return Err(invalid(format!("{at}.v must be finite (got {v})")));
            }
        }
        WaveformSpec::Pulse {
            delay,
            rise,
            width,
            fall,
            v,
        } => {
            finite_nonneg(&format!("{at}.delay"), *delay)?;
            finite_nonneg(&format!("{at}.rise"), *rise)?;
            finite_nonneg(&format!("{at}.width"), *width)?;
            finite_nonneg(&format!("{at}.fall"), *fall)?;
            if !v.is_finite() {
                return Err(invalid(format!("{at}.v must be finite (got {v})")));
            }
        }
        WaveformSpec::Pl { points } => {
            if points.is_empty() {
                return Err(invalid(format!("{at}.points must have at least one point")));
            }
            for (i, p) in points.iter().enumerate() {
                if !(p[0].is_finite() && p[1].is_finite()) {
                    return Err(invalid(format!(
                        "{at}.points[{i}] must be finite (got [{}, {}])",
                        p[0], p[1]
                    )));
                }
                if i > 0 && p[0] <= points[i - 1][0] {
                    return Err(invalid(format!(
                        "{at}.points times must be strictly increasing: points[{i}] has t = {} \
                         after t = {}",
                        p[0],
                        points[i - 1][0]
                    )));
                }
            }
        }
    }
    Ok(())
}

/// EQS-spec rules (scalar, before the mesh is read): the solve is a
/// real-material quasi-static one, so anything that only has meaning for a
/// full-wave solve is rejected rather than silently ignored.
pub(super) fn validate_eqs(spec: &ProblemSpec) -> Result<(), CliError> {
    let e = spec.eqs.as_ref().expect("eqs spec has an eqs section");
    if e.terminals.is_empty() {
        return Err(invalid(
            "eqs.terminals must name at least one driven terminal surface (a dimension-2 \
             physical group)",
        ));
    }
    let mut seen: Vec<&str> = Vec::new();
    for n in e
        .terminals
        .iter()
        .map(|t| t.name.as_str())
        .chain(e.floating.iter().map(String::as_str))
    {
        if seen.contains(&n) {
            return Err(invalid(format!(
                "eqs: `{n}` appears more than once among terminals / floating conductors"
            )));
        }
        seen.push(n);
    }
    match (&e.transient, &e.frequencies) {
        (Some(_), Some(_)) => {
            return Err(invalid(
                "eqs must have exactly one of `transient` and `frequencies`, not both",
            ));
        }
        (None, None) => {
            return Err(invalid(
                "eqs needs a mode: give exactly one of `transient` (time domain) and \
                 `frequencies` (small-signal admittance)",
            ));
        }
        (Some(t), None) => {
            for term in &e.terminals {
                match &term.waveform {
                    Some(w) => validate_waveform(&term.name, w)?,
                    None => {
                        return Err(invalid(format!(
                            "eqs.terminals[{}] has no `waveform`: a transient needs a drive \
                             waveform on every terminal",
                            term.name
                        )));
                    }
                }
            }
            if !(t.t_end.is_finite() && t.t_end > 0.0) {
                return Err(invalid(format!(
                    "eqs.transient.t_end must be finite and > 0 (got {})",
                    t.t_end
                )));
            }
            match (t.dt, t.n_steps) {
                (Some(_), Some(_)) => {
                    return Err(invalid(
                        "eqs.transient takes exactly one of `dt` and `n_steps`, not both",
                    ));
                }
                (None, None) => {
                    return Err(invalid(
                        "eqs.transient needs a step: give exactly one of `dt` and `n_steps`",
                    ));
                }
                (Some(dt), None) => {
                    if !(dt.is_finite() && dt > 0.0) {
                        return Err(invalid(format!(
                            "eqs.transient.dt must be finite and > 0 (got {dt})"
                        )));
                    }
                    let ratio = t.t_end / dt;
                    let n = ratio.round();
                    if (ratio - n).abs() > END_TIME_REL_TOL * ratio.max(1.0) {
                        return Err(invalid(format!(
                            "eqs.transient: t_end = {} is not an integer multiple of dt = {dt} \
                             (t_end / dt = {ratio}; nearest end times {} and {}) — adjust t_end \
                             or dt so the run reaches the requested end time",
                            t.t_end,
                            ratio.floor() * dt,
                            ratio.ceil() * dt
                        )));
                    }
                    if !(1.0..=MAX_EQS_STEPS as f64).contains(&n) {
                        return Err(invalid(format!(
                            "eqs.transient: t_end / dt = {} steps is outside [1, {MAX_EQS_STEPS}]",
                            t.t_end / dt
                        )));
                    }
                }
                (None, Some(n)) => {
                    if !(1..=MAX_EQS_STEPS).contains(&n) {
                        return Err(invalid(format!(
                            "eqs.transient.n_steps must be in [1, {MAX_EQS_STEPS}] (got {n})"
                        )));
                    }
                }
            }
            if !(0.5..=1.0).contains(&t.theta) {
                return Err(invalid(format!(
                    "eqs.transient.theta must be in [0.5, 1] (got {}); 1 is backward Euler, 0.5 \
                     Crank-Nicolson",
                    t.theta
                )));
            }
        }
        (None, Some(f)) => {
            if f.is_empty() {
                return Err(invalid("eqs.frequencies must list at least one frequency"));
            }
            if let Some(x) = f.iter().find(|x| !(x.is_finite() && **x >= 0.0)) {
                return Err(invalid(format!(
                    "eqs.frequencies must be finite and >= 0 Hz (got {x})"
                )));
            }
            if let Some(t) = e.terminals.iter().find(|t| t.waveform.is_some()) {
                return Err(invalid(format!(
                    "eqs.terminals[{}] has a `waveform`, which only applies to `transient` (the \
                     admittance drives each terminal with 1 V in turn)",
                    t.name
                )));
            }
        }
    }
    let static_only = |what: &str, why: &str| {
        invalid(format!(
            "an eqs spec cannot have {what}: {why} (`geode eqs` is a quasi-static scalar-potential \
             solve)"
        ))
    };
    if !spec.ports.is_empty() {
        return Err(static_only(
            "`ports`",
            "lumped ports are full-wave excitations; name the electrodes in `eqs.terminals`",
        ));
    }
    if !spec.wave_ports.is_empty() {
        return Err(static_only(
            "`wave_ports`",
            "wave ports are frequency-domain modal boundaries",
        ));
    }
    if spec.frequencies.is_some() {
        return Err(static_only(
            "a top-level `frequencies`",
            "give the frequencies as `eqs.frequencies`",
        ));
    }
    if !spec.absorbing_regions.is_empty() {
        return Err(static_only(
            "`absorbing_regions`",
            "UPML is a full-wave absorber",
        ));
    }
    let bcs = &spec.boundary_conditions;
    if !bcs.leontovich.is_empty() || !bcs.silver_muller.is_empty() || !bcs.pec.is_empty() {
        return Err(static_only(
            "`boundary_conditions` (pec / leontovich / silver_muller)",
            "terminals and floating conductors are the only conductor surfaces; every other \
             boundary is insulating",
        ));
    }
    if let SolverSpec::Iterative { .. } = spec.solver {
        return Err(invalid(
            "an eqs spec supports only `solver.mode = \"direct\"` (sparse LU)",
        ));
    }
    for m in &spec.materials {
        let g = &m.physical_group;
        if m.conductivity_s_m.is_none() {
            return Err(invalid(format!(
                "materials[{g}] has no `conductivity_s_m`: every material of an eqs spec needs \
                 its conductivity in S/m (0 for a pure dielectric)"
            )));
        }
        if m.dispersion.is_some() || m.eps_r_diag.is_some() || m.mu_r_diag.is_some() {
            return Err(invalid(format!(
                "materials[{g}]: `dispersion` / `eps_r_diag` / `mu_r_diag` are not supported by \
                 `geode eqs` (scalar real eps_r and conductivity_s_m only)"
            )));
        }
        if m.eps_r[1] != 0.0 {
            return Err(invalid(format!(
                "materials[{g}].eps_r has Im != 0; put the loss in `conductivity_s_m` (eps_r = \
                 [re, 0])"
            )));
        }
        if m.eps_r[0] <= 0.0 {
            return Err(invalid(format!(
                "materials[{g}].eps_r must have Re > 0 for the EQS solve"
            )));
        }
    }
    Ok(())
}

/// Bind the EQS terminals / floating conductors to their tagged triangles
/// and node sets and build the per-tet material arrays. Conductors that
/// share a mesh node are electrically shorted and rejected.
pub(super) fn resolve_eqs(
    e: &EqsSpec,
    terminal_tags: &[i32],
    floating_tags: &[i32],
    eps_f_m: Vec<f64>,
    sigma_s_m: Vec<f64>,
    regions: Vec<EqsRegionTarget>,
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<EqsTarget, CliError> {
    let conductor = |name: &str, tag: i32, role: &str| -> Result<Conductor, CliError> {
        let surface = surface(name, tag, role)?;
        let mut nodes: Vec<u32> = surface.triangles.iter().flatten().copied().collect();
        nodes.sort_unstable();
        nodes.dedup();
        Ok(Conductor { surface, nodes })
    };
    let terminals = e
        .terminals
        .iter()
        .zip(terminal_tags)
        .map(|(t, &tag)| conductor(&t.name, tag, "eqs terminal"))
        .collect::<Result<Vec<_>, _>>()?;
    let floating = e
        .floating
        .iter()
        .zip(floating_tags)
        .map(|(n, &tag)| conductor(n, tag, "eqs floating conductor"))
        .collect::<Result<Vec<_>, _>>()?;
    let all: Vec<&Conductor> = terminals.iter().chain(floating.iter()).collect();
    for (i, t) in all.iter().enumerate() {
        for u in &all[i + 1..] {
            let n = t
                .nodes
                .iter()
                .filter(|x| u.nodes.binary_search(x).is_ok())
                .count();
            if n > 0 {
                return Err(invalid(format!(
                    "eqs conductors `{}` and `{}` share {n} mesh node(s): touching conductors are \
                     electrically shorted — merge them into one or separate them in the mesh",
                    t.surface.name, u.surface.name
                )));
            }
        }
    }
    let waveforms = e.terminals.iter().map(|t| t.waveform.clone()).collect();
    let mode = match (&e.transient, &e.frequencies) {
        (Some(t), _) => {
            let (dt, n_steps) = match (t.dt, t.n_steps) {
                (Some(dt), _) => (dt, (t.t_end / dt).round() as usize),
                (None, Some(n)) => (t.t_end / n as f64, n),
                (None, None) => unreachable!("validated"),
            };
            EqsMode::Transient {
                dt,
                n_steps,
                theta: t.theta,
            }
        }
        (None, Some(f)) => EqsMode::Frequencies(f.clone()),
        (None, None) => unreachable!("validated"),
    };
    Ok(EqsTarget {
        terminals,
        floating,
        waveforms,
        eps_f_m,
        sigma_s_m,
        regions,
        mode,
    })
}
