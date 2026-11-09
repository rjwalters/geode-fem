//! Moved out of `problem/mod.rs` (issue #1398): pure move, no logic change.

use super::*;

/// `sensitivity`-section rules (scalar, before the mesh is read; issues
/// #707 / #739). v1 exposes only the observable × parameter pairs the library
/// has an FD-validated gradient for; everything else is rejected naming
/// the gap rather than silently ignored.
pub(super) fn validate_sensitivity(spec: &ProblemSpec, analysis: Analysis) -> Result<(), CliError> {
    let Some(sens) = &spec.sensitivity else {
        return Ok(());
    };
    if let Some(m) = spec
        .materials
        .iter()
        .find(|m| m.eps_r_diag.is_some() || m.mu_r_diag.is_some())
    {
        return Err(invalid(format!(
            "`sensitivity` does not support anisotropic materials in schema v1 \
             (materials[{}] has `eps_r_diag` / `mu_r_diag`): every library material \
             gradient differentiates a scalar per-region eps_r / nu_r on the scalar operator, \
             and there is no gradient with respect to a tensor component — drop \
             `sensitivity`, or use scalar materials",
            m.physical_group
        )));
    }
    match analysis {
        Analysis::Extract => {
            return Err(invalid(
                "`sensitivity` is not supported for an `extract` spec in schema v1: there is no \
                 Z / L0 / SRF / Q gradient — supported analyses are `capacitance` (one \
                 terminal), `inductance`, lossless `eigen` and one-lumped-port `driven` (|S11|²; \
                 drop the `extract` section and run `geode driven`)",
            ));
        }
        Analysis::Driven => {
            // Two paths (issue #883). The legacy one — one lumped port, every
            // parameter `eps_r`, no `observables` / `frequencies` — keeps the
            // port-loaded |S11|² adjoint (driven_material_adjoint_gradient_ports,
            // issue #739) and its report. Everything else runs the N-port
            // S-matrix adjoint (driven::s_sensitivity, Epic #841 Phase 1 / 3b),
            // which composes lumped, wave (geometric, filled), mixed, walled and
            // hybrid ports. Leontovich and Silver-Müller walls are admitted on
            // both: their term (iω/Z_s(ω))·S_Γ never depends on the volume ε.
            let lib = if is_n_port_sensitivity(spec) {
                "the N-port S-matrix adjoint (driven::s_sensitivity) differentiates the \
                 direct-LU, fixed scalar-ε operator"
            } else {
                "the library's port-loaded material adjoint \
                 (driven_material_adjoint_gradient_ports) differentiates |S11|² of one lumped \
                 port on the direct-LU, scalar-ε operator"
            };
            if !spec.absorbing_regions.is_empty() {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec does not support `absorbing_regions`: {lib}, \
                     while the matched UPML replaces each shell's ε by a frequency-dependent \
                     stretched tensor the adjoint does not differentiate (Epic #841 Phase 2a; use \
                     Silver-Müller walls for an open boundary)"
                )));
            }
            if spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec does not support `sweep.adaptive`: {lib} at \
                     every frequency, while the adaptive rows are reduced-order interpolations \
                     with no gradient (Epic #841 Phase 4) — remove `sweep.adaptive` to run the \
                     dense sweep"
                )));
            }
            if !matches!(spec.solver, SolverSpec::Direct {}) {
                return Err(invalid(format!(
                    "`sensitivity` on a driven spec needs `solver.mode = \"direct\"`: {lib} (one \
                     sparse LU serves the forward and the adjoint; adjoints stay direct-LU, an \
                     Epic #841 non-goal)"
                )));
            }
        }
        Analysis::Capacitance => {
            let n = spec.capacitance.as_ref().map_or(0, |c| c.terminals.len());
            if n != 1 {
                return Err(invalid(format!(
                    "`sensitivity` on a capacitance spec needs exactly one terminal (the \
                     two-terminal capacitance C between it and `ground`; got {n} terminals): the \
                     library's capacitance adjoint (capacitance_adjoint_gradient_p2) has no \
                     N-terminal Maxwell-matrix gradient"
                )));
            }
        }
        // Lossless and lossy / open pencils both differentiate (the lossy path
        // is the unconjugated Hellmann–Feynman of issue #1329).
        Analysis::Eigen => {}
        Analysis::Inductance => {}
    }
    if sens.parameters.is_empty() {
        return Err(invalid(
            "sensitivity.parameters must list at least one parameter",
        ));
    }
    for (i, prm) in sens.parameters.iter().enumerate() {
        use SensitivityParameterKind as K;
        let allowed: &[SensitivityParameterKind] = match analysis {
            Analysis::Inductance => &[K::NuR, K::MuR],
            Analysis::Driven => &[
                K::EpsR,
                K::EpsRImag,
                K::TanDelta,
                K::Sigma,
                K::Shape,
                K::WallSigma,
                K::WallRmsRoughness,
                K::WallBallRadius,
                K::WallBallDensity,
                K::Dispersion,
            ],
            _ => &[K::EpsR],
        };
        let label = parameter_label(prm);
        if !allowed.contains(&prm.kind) {
            let names: Vec<&str> = allowed.iter().map(|k| k.name()).collect();
            let why = if analysis != Analysis::Driven
                && !matches!(prm.kind, K::EpsR | K::NuR | K::MuR)
            {
                " (`eps_r_imag`, `tan_delta`, `sigma`, `shape` and the `wall_*` kinds are driven-spec \
                 parameters of the N-port S-matrix adjoint, Epic #841 Phase 5a; this analysis's \
                 adjoint has no loss, shape or wall gradient in the CLI yet)"
            } else {
                ""
            };
            return Err(invalid(format!(
                "sensitivity.parameters[{i}] ({label}): kind `{}` is not a parameter of the {} \
                 observable; supported kinds: `{}`{why}",
                prm.kind.name(),
                analysis.name(),
                names.join("`, `")
            )));
        }
        validate_parameter_fields(i, prm)?;
        let dup = sens.parameters[..i].iter().any(|q| {
            if prm.kind == K::Shape || q.kind == K::Shape {
                prm.kind == q.kind && parameter_label(q) == label
            } else if prm.kind == K::Dispersion || q.kind == K::Dispersion {
                prm.kind == q.kind
                    && q.physical_group == prm.physical_group
                    && q.model_parameter == prm.model_parameter
                    && q.pole == prm.pole
            } else {
                q.physical_group == prm.physical_group
                    && (q.kind == prm.kind || analysis == Analysis::Inductance)
            }
        });
        if dup {
            return Err(invalid(format!(
                "sensitivity.parameters lists {label} more than once"
            )));
        }
    }
    if analysis == Analysis::Driven {
        validate_observables(sens)?;
    } else if sens.observables.is_some() || sens.frequencies.is_some() {
        return Err(invalid(format!(
            "sensitivity.observables / sensitivity.frequencies apply to driven specs only (this \
             is a {} spec, whose observable is fixed)",
            analysis.name()
        )));
    }
    if analysis == Analysis::Eigen {
        let n_modes = spec.eigen.as_ref().map_or(0, |e| e.n_modes);
        if n_modes < 2 {
            return Err(invalid(format!(
                "`sensitivity` on an eigen spec needs eigen.n_modes ≥ 2 (got {n_modes}): the \
                 simple-eigenvalue check measures each differentiated mode's gap to the other \
                 returned modes"
            )));
        }
        let modes = sens.modes.clone().unwrap_or_else(|| vec![0]);
        if modes.is_empty() {
            return Err(invalid("sensitivity.modes must not be empty"));
        }
        for (i, &m) in modes.iter().enumerate() {
            if m >= n_modes {
                return Err(invalid(format!(
                    "sensitivity.modes[{i}] = {m} is out of range for eigen.n_modes = {n_modes}"
                )));
            }
            if modes[..i].contains(&m) {
                return Err(invalid(format!(
                    "sensitivity.modes lists mode {m} more than once"
                )));
            }
        }
        if let Some(g) = sens.min_rel_gap
            && !(g.is_finite() && g > 0.0)
        {
            return Err(invalid(format!(
                "sensitivity.min_rel_gap must be finite and > 0 (got {g})"
            )));
        }
    } else if sens.modes.is_some() || sens.min_rel_gap.is_some() {
        return Err(invalid(format!(
            "sensitivity.modes / sensitivity.min_rel_gap apply to eigen specs only (this is a \
             {} spec)",
            analysis.name()
        )));
    }
    if let Some(fd) = &sens.fd_check {
        if !(fd.relative_step.is_finite() && fd.relative_step > 0.0 && fd.relative_step <= 0.1) {
            return Err(invalid(format!(
                "sensitivity.fd_check.relative_step must be in (0, 0.1] (got {})",
                fd.relative_step
            )));
        }
        if !(fd.tolerance.is_finite() && fd.tolerance > 0.0) {
            return Err(invalid(format!(
                "sensitivity.fd_check.tolerance must be finite and > 0 (got {})",
                fd.tolerance
            )));
        }
    }
    Ok(())
}

/// Whether a driven spec's `sensitivity` runs the N-port S-matrix adjoint
/// (issue #883) rather than the legacy one-lumped-port `|S11|²` path: any
/// `observables` / `frequencies`, a parameter other than `eps_r`, or a port
/// set other than exactly one lumped port.
pub fn is_n_port_sensitivity(spec: &ProblemSpec) -> bool {
    let Some(sens) = &spec.sensitivity else {
        return false;
    };
    sens.observables.is_some()
        || sens.frequencies.is_some()
        || sens
            .parameters
            .iter()
            .any(|p| p.kind != SensitivityParameterKind::EpsR)
        || spec.ports.len() != 1
        || !spec.wave_ports.is_empty()
        // The legacy one-port adjoint differentiates a constant ε: a
        // dispersive material needs the N-port adjoint (issue #1259).
        || spec.materials.iter().any(|m| m.dispersion.is_some())
        // The legacy one-port adjoint has no σ term: a conducting material
        // needs the N-port adjoint (issue #1254).
        || spec
            .materials
            .iter()
            .any(|m| m.conductivity_s_m.is_some_and(|s| s > 0.0))
}

/// The report / message label of a sensitivity parameter: its volume group
/// (material kinds), or its name (shape; default `motion(groups)`).
pub fn parameter_label(prm: &crate::spec::SensitivityParameterSpec) -> String {
    if !prm.kind.is_shape() {
        return format!("`{}`", prm.physical_group.as_deref().unwrap_or("?"));
    }
    format!("`{}`", shape_name(prm))
}

/// The name of a shape parameter (its `name`, else `motion(groups)`).
pub fn shape_name(prm: &crate::spec::SensitivityParameterSpec) -> String {
    prm.name.clone().unwrap_or_else(|| {
        format!(
            "{}({})",
            prm.motion.map_or("shape", |m| m.name()),
            shape_groups(prm).join(",")
        )
    })
}

/// The moving groups of a shape parameter (`physical_group` then
/// `physical_groups`).
pub fn shape_groups(prm: &crate::spec::SensitivityParameterSpec) -> Vec<String> {
    prm.physical_group
        .iter()
        .chain(prm.physical_groups.iter().flatten())
        .cloned()
        .collect()
}

/// Field rules of one sensitivity parameter: a material kind names one
/// volume group and nothing else; a shape parameter names its groups, a
/// motion and a non-zero axis.
pub(super) fn validate_parameter_fields(
    i: usize,
    prm: &crate::spec::SensitivityParameterSpec,
) -> Result<(), CliError> {
    let shape_fields = [
        ("physical_groups", prm.physical_groups.is_some()),
        ("name", prm.name.is_some()),
        ("motion", prm.motion.is_some()),
        ("axis", prm.axis.is_some()),
        ("pinned", prm.pinned.is_some()),
    ];
    if prm.kind == crate::spec::SensitivityParameterKind::Dispersion {
        if prm.model_parameter.is_none() {
            return Err(invalid(format!(
                "sensitivity.parameters[{i}]: kind `dispersion` needs `model_parameter` (`eps_r`, \
                 `tan_delta`, `eps_inf`, `delta_eps`, `tau_s`, `omega_p_rad_s` or `gamma_rad_s`)"
            )));
        }
    } else if prm.model_parameter.is_some() || prm.pole.is_some() {
        return Err(invalid(format!(
            "sensitivity.parameters[{i}]: `model_parameter` / `pole` apply to `kind = \
             \"dispersion\"` parameters only (this one is `{}`)",
            prm.kind.name()
        )));
    }
    if !prm.kind.is_shape() {
        if prm.physical_group.is_none() {
            return Err(invalid(format!(
                "sensitivity.parameters[{i}]: kind `{}` needs `physical_group` ({})",
                prm.kind.name(),
                if prm.kind.is_wall() {
                    "a Leontovich wall's surface group"
                } else {
                    "a volume group"
                }
            )));
        }
        if let Some((f, _)) = shape_fields.iter().find(|(_, set)| *set) {
            return Err(invalid(format!(
                "sensitivity.parameters[{i}]: `{f}` applies to `kind = \"shape\"` parameters only \
                 (this one is `{}`)",
                prm.kind.name()
            )));
        }
        return Ok(());
    }
    let groups = shape_groups(prm);
    if groups.is_empty() {
        return Err(invalid(format!(
            "sensitivity.parameters[{i}] (shape): name the moving groups in `physical_group` or \
             `physical_groups`"
        )));
    }
    if prm.motion.is_none() {
        return Err(invalid(format!(
            "sensitivity.parameters[{i}] (shape `{}`): `motion` is required (`translate` or \
             `stretch`)",
            shape_name(prm)
        )));
    }
    match prm.axis {
        None => {
            return Err(invalid(format!(
                "sensitivity.parameters[{i}] (shape `{}`): `axis` is required (the motion's \
                 direction, e.g. [1, 0, 0])",
                shape_name(prm)
            )));
        }
        Some(a) if !(a.iter().all(|c| c.is_finite()) && a.iter().any(|&c| c != 0.0)) => {
            return Err(invalid(format!(
                "sensitivity.parameters[{i}] (shape `{}`): `axis` must be finite and non-zero \
                 (got {a:?})",
                shape_name(prm)
            )));
        }
        Some(_) => {}
    }
    if let Some(g) = prm.pinned.iter().flatten().find(|g| groups.contains(g)) {
        return Err(invalid(format!(
            "sensitivity.parameters[{i}] (shape `{}`): group `{g}` both moves and is pinned",
            shape_name(prm)
        )));
    }
    Ok(())
}

/// Field rules of the driven `observables` (scalar; indices are checked
/// once the ports are resolved).
pub(super) fn validate_observables(sens: &crate::spec::SensitivitySpec) -> Result<(), CliError> {
    use crate::spec::{ObservableForm as F, ObservableQuantity as Q};
    let Some(obs) = &sens.observables else {
        return Ok(());
    };
    if obs.is_empty() {
        return Err(invalid(
            "sensitivity.observables must list at least one observable (or be omitted)",
        ));
    }
    for (i, o) in obs.iter().enumerate() {
        let at = format!("sensitivity.observables[{i}] (`{}`)", o.quantity.name());
        let only = |field: &str, set: bool| -> Result<(), CliError> {
            if set {
                return Err(invalid(format!(
                    "{at}: `{field}` does not apply to this quantity"
                )));
            }
            Ok(())
        };
        match o.quantity {
            Q::S => {
                if o.entry.is_none() {
                    return Err(invalid(format!("{at}: `entry` ([i, j]) is required")));
                }
                only("entries", o.entries.is_some())?;
                only("wave_port", o.wave_port.is_some())?;
                only("mode", o.mode.is_some())?;
            }
            Q::SSumSq => {
                match &o.entries {
                    None => return Err(invalid(format!("{at}: `entries` is required"))),
                    Some(e) if e.is_empty() => {
                        return Err(invalid(format!("{at}: `entries` must not be empty")));
                    }
                    Some(e) => {
                        if let Some((k, x)) =
                            e.iter().enumerate().find(|(k, x)| e[..*k].contains(x))
                        {
                            return Err(invalid(format!(
                                "{at}: entries[{k}] = {x:?} is listed more than once"
                            )));
                        }
                    }
                }
                only("entry", o.entry.is_some())?;
                only("wave_port", o.wave_port.is_some())?;
                only("mode", o.mode.is_some())?;
                if o.form.is_some() {
                    return Err(invalid(format!(
                        "{at}: `form` does not apply (the sum of |S_ij|² is real)"
                    )));
                }
            }
            Q::Z0 | Q::EpsEff => {
                if o.wave_port.is_none() {
                    return Err(invalid(format!(
                        "{at}: `wave_port` (a hybrid wave port's physical_group) is required"
                    )));
                }
                only("entry", o.entry.is_some())?;
                only("entries", o.entries.is_some())?;
                if let Some(f) = o.form
                    && !matches!(f, F::Real | F::Imag | F::Mag)
                {
                    return Err(invalid(format!(
                        "{at}: form `{}` is not supported for a port-mode quantity (use `real`, \
                         `imag` or `mag`)",
                        f.name()
                    )));
                }
            }
        }
    }
    Ok(())
}

/// What [`resolve_sensitivity`] needs of the rest of the resolved spec.
pub(super) struct SensitivityContext<'a> {
    /// The N-port path ([`is_n_port_sensitivity`]).
    pub(super) n_port: bool,
    /// Lumped-port count (the first S channels).
    pub(super) n_lumped: usize,
    /// The resolved wave ports.
    pub(super) wave_ports: &'a [WavePortDef],
    /// The swept frequencies.
    pub(super) frequencies: &'a [Frequency],
    /// Metres per mesh unit.
    pub(super) length_unit_m: f64,
    /// The resolved Leontovich walls (wall parameters, issue #1330).
    pub(super) leontovich: &'a [Leontovich],
    /// The dispersive regions (`kind = "dispersion"` parameters, #1259).
    pub(super) dispersion: &'a [DispersiveRegion],
}

/// Bind each sensitivity parameter to its volume region (the regions are
/// sorted by tag, and every region is a design region), resolve shape
/// parameters' motions, and (driven) the observables and frequencies.
pub(super) fn resolve_sensitivity(
    sens: &crate::spec::SensitivitySpec,
    tags: &[Option<i32>],
    regions: &[Region],
    tagged: &TaggedTetMesh,
    analysis: Analysis,
    ctx: &SensitivityContext<'_>,
) -> Result<SensitivityTarget, CliError> {
    let index_of: BTreeMap<i32, usize> = regions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.tag, i))
        .collect();
    let parameters = sens
        .parameters
        .iter()
        .zip(tags)
        .map(|(prm, tag)| {
            if prm.kind.is_shape() {
                return resolve_shape_parameter(prm, tagged, ctx.length_unit_m);
            }
            if prm.kind.is_wall() {
                return resolve_wall_parameter(prm, ctx.leontovich, ctx.length_unit_m);
            }
            let group = prm.physical_group.clone().expect("validated");
            let tag = tag.expect("resolved above");
            let region = *index_of.get(&tag).ok_or_else(|| {
                invalid(format!(
                    "sensitivity parameter `{group}` names a volume group with no tets"
                ))
            })?;
            let r = &regions[region];
            if prm.kind == SensitivityParameterKind::Dispersion {
                return resolve_dispersion_parameter(prm, &group, region, tag, ctx.dispersion);
            }
            if ctx.dispersion.iter().any(|d| d.tag == tag) {
                return Err(invalid(format!(
                    "sensitivity parameter `{group}` ({}): the group has a `dispersion` model, \
                     whose permittivity is ε(ω) rather than a constant — differentiate its \
                     model parameters with `kind = \"dispersion\"` (`model_parameter`)",
                    prm.kind.name()
                )));
            }
            let value = match prm.kind {
                SensitivityParameterKind::EpsR => r.eps_r.re,
                SensitivityParameterKind::EpsRImag => -r.eps_r.im,
                SensitivityParameterKind::TanDelta => -r.eps_r.im / r.eps_r.re,
                SensitivityParameterKind::NuR => 1.0 / r.mu_r,
                SensitivityParameterKind::MuR => r.mu_r,
                SensitivityParameterKind::Sigma => r.sigma_s_m,
                _ => unreachable!("handled above"),
            };
            Ok(SensitivityParameter {
                physical_group: group,
                kind: prm.kind,
                region,
                value,
                shape: None,
                wall: None,
                dispersion: None,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let region_of_tet = tagged
        .tet_physical_tags
        .iter()
        .map(|t| index_of[t])
        .collect();
    let (observables, frequency_indices) = if analysis == Analysis::Driven && ctx.n_port {
        (
            resolve_observables(sens, ctx)?,
            resolve_sensitivity_frequencies(sens, ctx)?,
        )
    } else {
        (Vec::new(), (0..ctx.frequencies.len()).collect())
    };
    Ok(SensitivityTarget {
        parameters,
        region_of_tet,
        n_regions: regions.len(),
        modes: if analysis == Analysis::Eigen {
            sens.modes.clone().unwrap_or_else(|| vec![0])
        } else {
            Vec::new()
        },
        min_rel_gap: sens.min_rel_gap.unwrap_or(DEFAULT_SENSITIVITY_MIN_REL_GAP),
        fd_check: sens
            .fd_check
            .as_ref()
            .map(|fd| (fd.relative_step, fd.tolerance)),
        n_port: analysis == Analysis::Driven && ctx.n_port,
        observables,
        frequency_indices,
    })
}

/// A wall parameter's wall, library parameter, SI value and unit factor
/// (issue #1330). Rejects a group that is not a Leontovich wall and a
/// parameter the wall's model does not carry.
pub(super) fn resolve_wall_parameter(
    prm: &crate::spec::SensitivityParameterSpec,
    leontovich: &[Leontovich],
    length_unit_m: f64,
) -> Result<SensitivityParameter, CliError> {
    use geode_core::driven::solve::WallParam;
    let group = prm.physical_group.clone().expect("validated");
    let label = parameter_label(prm);
    let at = format!("sensitivity parameter {label} (kind `{}`)", prm.kind.name());
    let wall = leontovich
        .iter()
        .position(|l| l.surface.name == group)
        .ok_or_else(|| {
            invalid(format!(
                "{at}: `{group}` is not a `boundary_conditions.leontovich` wall (wall parameters \
                 name the wall's dimension-2 `physical_group`)"
            ))
        })?;
    let l = &leontovich[wall];
    let needs = |what: &str| {
        invalid(format!(
            "{at}: wall `{group}` {what}; the parameter does not exist on it"
        ))
    };
    let (param, value, si_factor, unit) = match (prm.kind, l.roughness) {
        (SensitivityParameterKind::WallSigma, _) => (
            WallParam::Sigma,
            l.sigma_s_m,
            ETA_0_OHM * length_unit_m,
            "S/m",
        ),
        (SensitivityParameterKind::WallRmsRoughness, Some(RoughnessSpec::Hammerstad { rms_m })) => {
            (WallParam::RmsRoughness, rms_m, 1.0 / length_unit_m, "m")
        }
        (SensitivityParameterKind::WallRmsRoughness, None) => {
            return Err(needs("is smooth (no `roughness`)"));
        }
        (SensitivityParameterKind::WallRmsRoughness, Some(_)) => {
            return Err(needs(
                "has a Huray roughness (use `wall_ball_radius` / `wall_ball_density`)",
            ));
        }
        (
            SensitivityParameterKind::WallBallRadius,
            Some(RoughnessSpec::Huray { ball_radius_m, .. }),
        ) => (
            WallParam::BallRadius,
            ball_radius_m,
            1.0 / length_unit_m,
            "m",
        ),
        (
            SensitivityParameterKind::WallBallDensity,
            Some(RoughnessSpec::Huray {
                n_balls,
                tile_area_m2,
                ..
            }),
        ) => (
            WallParam::BallDensity,
            n_balls / tile_area_m2,
            length_unit_m * length_unit_m,
            "1/m^2",
        ),
        (
            SensitivityParameterKind::WallBallRadius | SensitivityParameterKind::WallBallDensity,
            None,
        ) => {
            return Err(needs("is smooth (no `roughness`)"));
        }
        (
            SensitivityParameterKind::WallBallRadius | SensitivityParameterKind::WallBallDensity,
            Some(_),
        ) => {
            return Err(needs(
                "has a Hammerstad roughness (use `wall_rms_roughness`)",
            ));
        }
        _ => unreachable!("wall kinds only"),
    };
    if !(value.is_finite() && value > 0.0) {
        return Err(invalid(format!(
            "{at}: the parameter's value is {value:e}; the finite-difference scale and the \
             gradient need a positive value"
        )));
    }
    Ok(SensitivityParameter {
        physical_group: group,
        kind: prm.kind,
        region: usize::MAX,
        value,
        shape: None,
        dispersion: None,
        wall: Some(WallParameterDef {
            wall,
            param,
            si_factor,
            unit,
        }),
    })
}

/// A `kind = "dispersion"` parameter: the named parameter of the group's
/// model, at its design value (issue #1259).
pub(super) fn resolve_dispersion_parameter(
    prm: &crate::spec::SensitivityParameterSpec,
    group: &str,
    region: usize,
    tag: i32,
    dispersion: &[DispersiveRegion],
) -> Result<SensitivityParameter, CliError> {
    use crate::spec::{DispersionParameter as P, DispersionSpec as D};
    let param = prm.model_parameter.expect("validated");
    let d = dispersion.iter().find(|d| d.tag == tag).ok_or_else(|| {
        invalid(format!(
            "sensitivity parameter `{group}` (dispersion): the group has no \
             `materials[].dispersion` model (a constant `eps_r` is differentiated with `eps_r`, \
             `eps_r_imag`, `tan_delta`)"
        ))
    })?;
    let bad = |why: String| {
        invalid(format!(
            "sensitivity parameter `{group}` (dispersion, `{}`): {why}",
            param.name()
        ))
    };
    let (value, pole) = match (&d.spec, param) {
        (D::DjordjevicSarkar { eps_r, .. }, P::EpsR) => (*eps_r, 0),
        (D::DjordjevicSarkar { tan_delta, .. }, P::TanDelta) => (*tan_delta, 0),
        (D::Debye { eps_inf, .. } | D::Drude { eps_inf, .. }, P::EpsInf) => (*eps_inf, 0),
        (D::Debye { poles, .. }, P::DeltaEps | P::TauS) => {
            let k = prm.pole.ok_or_else(|| {
                bad("a Debye pole parameter needs `pole` (the 0-based pole index)".into())
            })?;
            let pole = poles.get(k).ok_or_else(|| {
                bad(format!(
                    "`pole` = {k} is out of range: the model has {} pole(s)",
                    poles.len()
                ))
            })?;
            (
                if param == P::DeltaEps {
                    pole.delta_eps
                } else {
                    pole.tau_s
                },
                k,
            )
        }
        (D::Drude { omega_p_rad_s, .. }, P::OmegaPRadS) => (*omega_p_rad_s, 0),
        (D::Drude { gamma_rad_s, .. }, P::GammaRadS) => (*gamma_rad_s, 0),
        _ => {
            return Err(bad(format!(
                "the group's `{}` model has no such parameter",
                d.model.name()
            )));
        }
    };
    if prm.pole.is_some() && !matches!(param, P::DeltaEps | P::TauS) {
        return Err(bad("`pole` applies to `delta_eps` / `tau_s` only".into()));
    }
    Ok(SensitivityParameter {
        physical_group: group.to_string(),
        kind: SensitivityParameterKind::Dispersion,
        region,
        value,
        shape: None,
        wall: None,
        dispersion: Some(DispersionParameterDef { param, pole }),
    })
}

/// A shape parameter's motion, value and FD length (issue #883).
pub(super) fn resolve_shape_parameter(
    prm: &crate::spec::SensitivityParameterSpec,
    tagged: &TaggedTetMesh,
    length_unit_m: f64,
) -> Result<SensitivityParameter, CliError> {
    use geode_core::driven::s_sensitivity::{GroupMotion, GroupMotionKind};
    let name = shape_name(prm);
    let groups = shape_groups(prm);
    let raw = prm.axis.expect("validated");
    let n = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
    let axis = [raw[0] / n, raw[1] / n, raw[2] / n];
    let motion = prm.motion.expect("validated");
    let gm = GroupMotion {
        name: name.clone(),
        groups: groups.clone(),
        kind: match motion {
            ShapeMotion::Translate => GroupMotionKind::Translate { dir: axis },
            ShapeMotion::Stretch => GroupMotionKind::Stretch { axis },
        },
    };
    let bad = |e: geode_core::driven::s_sensitivity::SSensitivityError| {
        invalid(format!("sensitivity shape parameter `{name}`: {e}"))
    };
    let value_mesh = gm.value(tagged).map_err(bad)?;
    // FD length: the moving nodes' bounding-box size (the stretch extent
    // for a stretch).
    let nodes = gm.nodes(tagged).map_err(bad)?;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &v in &nodes {
        let x = tagged.mesh.nodes[v as usize];
        for k in 0..3 {
            lo[k] = lo[k].min(x[k]);
            hi[k] = hi[k].max(x[k]);
        }
    }
    let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f64::max);
    let fd_length = if value_mesh > 0.0 { value_mesh } else { size };
    if fd_length.is_nan() || fd_length <= 0.0 {
        return Err(invalid(format!(
            "sensitivity shape parameter `{name}`: its groups are a single point (no length \
             scale for the motion)"
        )));
    }
    Ok(SensitivityParameter {
        physical_group: groups.join(","),
        kind: SensitivityParameterKind::Shape,
        region: usize::MAX,
        wall: None,
        dispersion: None,
        value: value_mesh * length_unit_m,
        shape: Some(ShapeParameterDef {
            name,
            motion: gm,
            shape_motion: motion,
            pinned: prm.pinned.clone().unwrap_or_default(),
            fd_length,
        }),
    })
}

/// The driven `observables` (issue #883), defaulting to every `|S_ij|²`.
pub(super) fn resolve_observables(
    sens: &crate::spec::SensitivitySpec,
    ctx: &SensitivityContext<'_>,
) -> Result<Vec<ObservableDef>, CliError> {
    use crate::spec::{ObservableForm as F, ObservableQuantity as Q};
    let n = ctx.n_lumped + ctx.wave_ports.iter().map(|w| w.a_inc.len()).sum::<usize>();
    let Some(obs) = &sens.observables else {
        return Ok((0..n * n)
            .map(|k| ObservableDef {
                quantity: Q::S,
                entries: vec![[k / n, k % n]],
                form: F::MagSq,
                wave_port: None,
                mode: 0,
                label: format!("|S[{},{}]|^2", k / n, k % n),
                unit: "1",
            })
            .collect());
    };
    let check = |at: &str, e: [usize; 2]| -> Result<(), CliError> {
        if e[0] >= n || e[1] >= n {
            return Err(invalid(format!(
                "{at}: entry {e:?} is out of range for the {n}-channel S matrix (lumped ports \
                 first, then the wave channels port-major, mode-minor)"
            )));
        }
        Ok(())
    };
    obs.iter()
        .enumerate()
        .map(|(i, o)| {
            let at = format!("sensitivity.observables[{i}] (`{}`)", o.quantity.name());
            Ok(match o.quantity {
                Q::S => {
                    let e = o.entry.expect("validated");
                    check(&at, e)?;
                    let form = o.form.unwrap_or(F::MagSq);
                    ObservableDef {
                        quantity: o.quantity,
                        entries: vec![e],
                        form,
                        wave_port: None,
                        mode: 0,
                        label: format!("{}(S[{},{}])", form.name(), e[0], e[1]),
                        unit: form_unit(form, "1"),
                    }
                }
                Q::SSumSq => {
                    let entries = o.entries.clone().expect("validated");
                    for &e in &entries {
                        check(&at, e)?;
                    }
                    let list: Vec<String> = entries
                        .iter()
                        .map(|e| format!("{},{}", e[0], e[1]))
                        .collect();
                    ObservableDef {
                        quantity: o.quantity,
                        entries,
                        form: F::MagSq,
                        wave_port: None,
                        mode: 0,
                        label: format!("sum |S[{}]|^2", list.join("; ")),
                        unit: "1",
                    }
                }
                Q::Z0 | Q::EpsEff => {
                    let name = o.wave_port.as_deref().expect("validated");
                    let k = ctx
                        .wave_ports
                        .iter()
                        .position(|w| w.surface.name == name)
                        .ok_or_else(|| {
                            invalid(format!(
                                "{at}: `{name}` is not one of the spec's wave ports"
                            ))
                        })?;
                    let w = &ctx.wave_ports[k];
                    let Some(h) = &w.hybrid else {
                        return Err(invalid(format!(
                            "{at}: wave port `{name}` is a geometric (homogeneous TE) port: its \
                             β and wave impedance are closed-form in the fill, and only a hybrid \
                             port (an inhomogeneous face or a floating conductor) has a 2-D \
                             port-mode `{}` gradient",
                            o.quantity.name()
                        )));
                    };
                    if o.quantity == Q::Z0 && h.impedance_definition.is_none() {
                        return Err(invalid(format!(
                            "{at}: hybrid wave port `{name}` carries no floating conductor, so \
                             no line impedance exists — differentiate `eps_eff` or an S entry"
                        )));
                    }
                    let mode = o.mode.unwrap_or(0);
                    if mode >= w.a_inc.len() {
                        return Err(invalid(format!(
                            "{at}: mode {mode} is out of range for wave port `{name}`'s {} \
                             reported channel(s)",
                            w.a_inc.len()
                        )));
                    }
                    let form = o.form.unwrap_or(F::Real);
                    let base = if o.quantity == Q::Z0 { "ohm" } else { "1" };
                    ObservableDef {
                        quantity: o.quantity,
                        entries: Vec::new(),
                        form,
                        wave_port: Some(k),
                        mode,
                        label: format!(
                            "{}({}[{name}, mode {mode}])",
                            form.name(),
                            o.quantity.name()
                        ),
                        unit: form_unit(form, base),
                    }
                }
            })
        })
        .collect()
}

/// Unit of a form applied to a quantity of unit `base`.
pub(super) fn form_unit(form: crate::spec::ObservableForm, base: &'static str) -> &'static str {
    use crate::spec::ObservableForm as F;
    match (form, base) {
        (F::Db, _) => "dB",
        (F::PhaseDeg, _) => "deg",
        (F::MagSq, "ohm") => "ohm^2",
        (_, b) => b,
    }
}

/// Indices into the swept frequencies of `sensitivity.frequencies` (all by
/// default), ascending.
pub(super) fn resolve_sensitivity_frequencies(
    sens: &crate::spec::SensitivitySpec,
    ctx: &SensitivityContext<'_>,
) -> Result<Vec<usize>, CliError> {
    let Some(fs) = &sens.frequencies else {
        return Ok((0..ctx.frequencies.len()).collect());
    };
    let values = fs
        .expand()
        .map_err(|e| invalid(format!("sensitivity.frequencies: {e}")))?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        let f = to_frequency(v, fs.unit, ctx.length_unit_m);
        let i = ctx
            .frequencies
            .iter()
            .position(|g| (g.hz - f.hz).abs() <= 1e-9 * f.hz)
            .ok_or_else(|| {
                invalid(format!(
                    "sensitivity.frequencies: {v} ({:e} Hz) is not one of the swept \
                     `frequencies` (the gradients are taken on the sweep's own solves)",
                    f.hz
                ))
            })?;
        if out.contains(&i) {
            return Err(invalid(format!(
                "sensitivity.frequencies lists {v} ({:e} Hz) more than once",
                f.hz
            )));
        }
        out.push(i);
    }
    out.sort_unstable();
    Ok(out)
}
