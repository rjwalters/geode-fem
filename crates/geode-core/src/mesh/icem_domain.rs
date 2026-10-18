//! **ICEM reference motor: the geometry domain of the construction**
//! (motor paper experiment 2, #1185 amendment 7 §1, issue #1201).
//!
//! [`super::icem_parametric::sector_curves`] and
//! [`super::icem_controlpoints::sector_curves_cp`] refuse a design whose
//! construction degenerates. This module turns each of those refusals into a
//! **raw margin** `m_i` with a unit, so that an optimizer can see how far a
//! design is from the edge of the buildable set, and the derivative of the
//! margin with respect to the design.
//!
//! # Inventory
//!
//! Every guard of the two constructions, in their order. The *smooth*
//! guards are inequalities on a differentiable margin; the construction
//! passes a strict guard when `m > 0` and a non-strict one when `m ≥ 0`,
//! exactly as it compares (no epsilon is added).
//!
//! | guard | margin `m` | unit | test | source |
//! |---|---|---|---|---|
//! | `magnet_shaft_clearance` | `R_r − MAG − MH − R_shaft` | m | `> 0` | `sector_curves`, magnet guard |
//! | `magnet_height` | `MH` | m | `> 0` | `sector_curves`, magnet guard |
//! | `magnet_half_width` | `MW / 2` | m | `> 0` | `sector_curves`, magnet guard |
//! | `opening_half_width` | `SW2 / 2` | m | `> 0` | `sector_curves`, opening guard |
//! | `opening_vs_pitch` | `R_b sin α − SW2 / 2` | m | `> 0` | `sector_curves`, opening guard |
//! | `slot_top_discriminant` | `q_b² − q_a q_c` | m² | `≥ 0` | `sector_curves`, slot-top arc |
//! | `arc_centre_beyond_opening` | `u − x_c` | m | `> 0` | `sector_curves`, slot-top arc |
//! | `arc_radius_vs_opening` | `ρ − SW2 / 2` | m | `> 0` | `sector_curves`, slot-top arc |
//! | `fillet_radius` | `Sr1` | m | `> 0` | `sector_curves`, flank guard |
//! | `fillet_centre_y` | `y_f` | m | `> 0` | `sector_curves`, flank guard |
//! | `flank_length` | `f_hi.x − t_hi.x` | m | `> 0` | `sector_curves`, flank guard |
//! | `cp_coupling.<label>` (55, space B) | `r_c − |P_i|` (rotor), `|P_i| − r_c` (stator) | m | `> 0` | `sector_curves_cp`, coupling-circle guard |
//!
//! The one *discrete* guard is `components_finite`: every component is a
//! finite number (the construction's `is_nan` test on the discriminant, and
//! `FrozenSpace::embed` in the harness). It has no gradient. The choice of
//! the larger root of the slot-top quadratic is a fixed rule of the
//! construction, not a pass/fail test; its validity is the smooth guard
//! `arc_centre_beyond_opening`.
//!
//! # Safe evaluation
//!
//! Each guard is evaluated **before** the operation that is undefined
//! outside it, and a margin that depends on such an operation is computed
//! only when its prerequisite guards pass ([`GuardSpec::requires`]):
//!
//! - `asin(SW2 / (2 R_b))` needs both opening guards: the discriminant,
//!   `u`, `ρ`, the flank and the stator control points use it;
//! - `√disc` needs the discriminant guard: `u`, `ρ` and the flank use it.
//!
//! A margin whose prerequisite fails is **undefined**. Its safe value is
//! `c = −1` (so `h = −1 − τ`) with a zero derivative, and it fails its
//! guard. A violated guard is therefore reported through its own raw margin,
//! never through a NaN raised further downstream. Such a design is never
//! buildable, so the safe values only reach a method as constraint values
//! at a point that is refused before any mesh or solve.
//!
//! # Derivative domain
//!
//! At a discriminant of exactly zero the geometry exists (the guard is
//! non-strict), but `d√disc` is infinite, so the derivatives of `u`, `ρ`
//! and the flank (and the shape adjoint through the curves) are not
//! defined. This module does not invent a finite value: those margins carry
//! no derivative, [`Domain::jacobian_defined`] is `false`, and the harness
//! refuses any gradient request there as a recorded derivative-domain
//! rejection. Any other non-finite derivative is treated the same way.
//!
//! # Scaling and thresholds
//!
//! Each smooth margin has a fixed positive scale `s_i` of its own dimension:
//! `s_i = |m_i(θ0)|` at the registered initial design, or, where that is
//! zero, a stated value ([`ZERO_REFERENCE_SCALES`], empty: no guard of
//! either space has a zero margin at θ0). The scaled margin is
//! `c_i = m_i / s_i` and the constraint the optimizers see is
//! `h_i = c_i − τ ≥ 0` with `τ = 0.05` ([`TAU`]).
//!
//! - **Buildable**: the construction succeeds (every guard passes as it
//!   compares) and every discrete guard passes. Only buildable designs may
//!   be meshed or solved.
//! - **Admissible**: buildable and every `h_i ≥ 0`.
//!
//! τ is a prospective modelling choice, not a verified safety margin; the
//! physical floor it implies for guard `i` is `τ s_i`.

use super::icem_controlpoints::{
    CpPart, IcemCpDesign, N_CP_PARAMS, N_ROTOR_POINTS, SW2_GENERATOR_DEFAULT,
    control_point_positions, control_points, cp_param_names, sector_curves_cp,
};
use super::icem_parametric::{
    DualN, HALF_SLOT, IcemDesign, N_PARAMS, PARAM_NAMES, R_BORE, R_COUPLING, R_ROTOR, R_SHAFT,
    Real, sector_curves,
};

/// The admissibility threshold on the scaled margins: `h = c − τ`.
pub const TAU: f64 = 0.05;

/// Guards whose margin is zero at θ0, with the stated scale (in the
/// guard's unit) and its justification. Empty: every smooth guard of both
/// spaces has a nonzero θ0 margin (checked by the tests), so every scale is
/// `|m_i(θ0)|`.
pub const ZERO_REFERENCE_SCALES: &[(&str, f64, &str)] = &[];

/// The unit of a margin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginUnit {
    /// Metres.
    Length,
    /// Square metres.
    Area,
}

impl MarginUnit {
    /// `"m"` or `"m^2"`.
    pub fn symbol(self) -> &'static str {
        match self {
            MarginUnit::Length => "m",
            MarginUnit::Area => "m^2",
        }
    }
}

/// One smooth guard of the inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardSpec {
    /// Stable name.
    pub name: String,
    /// The margin's unit (its scale has the same one).
    pub unit: MarginUnit,
    /// `true`: passes when `m > 0`; `false`: when `m ≥ 0`.
    pub strict: bool,
    /// Guards that must pass before this margin is defined.
    pub requires: &'static [&'static str],
    /// What the margin is, and where the construction tests it.
    pub source: &'static str,
}

/// The evaluation of one smooth guard at a design.
#[derive(Clone, Debug, PartialEq)]
pub struct GuardEval {
    pub spec: GuardSpec,
    /// Raw margin `m_i` (`None`: undefined, a prerequisite failed).
    pub margin: Option<f64>,
    /// The fixed scale `s_i` (the margin's unit).
    pub scale: f64,
    /// `c_i = m_i / s_i`, or the safe value `−1` when undefined.
    pub c: f64,
    /// `h_i = c_i − τ`.
    pub h: f64,
    /// `∂h_i/∂θ` over the geometric components ([`PARAM_NAMES`], or
    /// [`cp_param_names`] for space B); zero when undefined; `None` where
    /// the derivative is not defined (module docs).
    pub grad_h: Option<Vec<f64>>,
    /// Whether the guard passes as the construction tests it.
    pub pass: bool,
}

/// The evaluation of one discrete guard.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscreteEval {
    pub name: &'static str,
    pub pass: bool,
    pub source: &'static str,
}

/// The geometry domain at one design (module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct Domain {
    /// Names of the geometric components the gradients are over.
    pub components: Vec<&'static str>,
    pub guards: Vec<GuardEval>,
    pub discrete: Vec<DiscreteEval>,
    /// The construction's own verdict (`Err`: its message).
    pub construction: Result<(), String>,
}

impl Domain {
    /// Every smooth guard passes as the construction tests it.
    pub fn guards_pass(&self) -> bool {
        self.guards.iter().all(|g| g.pass)
    }

    /// Every discrete guard passes.
    pub fn discrete_pass(&self) -> bool {
        self.discrete.iter().all(|d| d.pass)
    }

    /// The construction succeeds and every discrete guard passes. (The
    /// smooth guards agree with the construction; the tests check it.)
    pub fn buildable(&self) -> bool {
        self.construction.is_ok() && self.discrete_pass() && self.guards_pass()
    }

    /// Buildable and every `h_i ≥ 0`.
    pub fn admissible(&self) -> bool {
        self.buildable() && self.guards.iter().all(|g| g.h >= 0.0)
    }

    /// The smallest `h_i` (`−∞` without guards).
    pub fn min_h(&self) -> f64 {
        self.guards
            .iter()
            .map(|g| g.h)
            .fold(f64::INFINITY, f64::min)
    }

    /// Every guard has a derivative.
    pub fn jacobian_defined(&self) -> bool {
        self.guards.iter().all(|g| g.grad_h.is_some())
    }

    /// The names of the guards with `h_i < 0`.
    pub fn inadmissible_guards(&self) -> Vec<&str> {
        self.guards
            .iter()
            .filter(|g| g.h < 0.0)
            .map(|g| g.spec.name.as_str())
            .collect()
    }
}

const MAGNET: &str = "magnet guard: x_in > R_SHAFT && MH > 0 && MW/2 > 0";
const OPENING: &str = "opening guard: SW2/2 > 0 && SW2/2 < R_BORE sin(alpha)";
const DISC: &str = "slot-top arc: discriminant not NaN and >= 0 (before the square root)";
const ARC: &str = "slot-top arc: u > x_c && rho > SW2/2";
const FLANK: &str = "flank guard: Sr1 > 0 && y_f > 0 && f_hi.x > t_hi.x";
const CP: &str = "sector_curves_cp: rotor point |P| < r_c, stator point |P| > r_c";
const OPENING_REQ: &[&str] = &["opening_half_width", "opening_vs_pitch"];
const DISC_REQ: &[&str] = &[
    "opening_half_width",
    "opening_vs_pitch",
    "slot_top_discriminant",
];

fn spec(
    name: &str,
    unit: MarginUnit,
    strict: bool,
    requires: &'static [&'static str],
    source: &'static str,
) -> GuardSpec {
    GuardSpec {
        name: name.to_string(),
        unit,
        strict,
        requires,
        source,
    }
}

/// The smooth guards of the 9-parameter construction, in order.
pub fn scalar_guard_specs() -> Vec<GuardSpec> {
    use MarginUnit::{Area, Length};
    vec![
        spec("magnet_shaft_clearance", Length, true, &[], MAGNET),
        spec("magnet_height", Length, true, &[], MAGNET),
        spec("magnet_half_width", Length, true, &[], MAGNET),
        spec("opening_half_width", Length, true, &[], OPENING),
        spec("opening_vs_pitch", Length, true, &[], OPENING),
        spec("slot_top_discriminant", Area, false, OPENING_REQ, DISC),
        spec("arc_centre_beyond_opening", Length, true, DISC_REQ, ARC),
        spec("arc_radius_vs_opening", Length, true, DISC_REQ, ARC),
        spec("fillet_radius", Length, true, &[], FLANK),
        spec("fillet_centre_y", Length, true, &[], FLANK),
        spec("flank_length", Length, true, DISC_REQ, FLANK),
    ]
}

/// The smooth guards of the control-point construction: the scalar ones,
/// then one coupling-circle guard per control point (`control_points`
/// order).
pub fn cp_guard_specs() -> Vec<GuardSpec> {
    let mut out = scalar_guard_specs();
    for cp in control_points() {
        let requires: &'static [&'static str] = match cp.part {
            CpPart::Rotor => &[],
            CpPart::Stator => OPENING_REQ,
        };
        out.push(spec(
            &format!("cp_coupling.{}", cp.label),
            MarginUnit::Length,
            true,
            requires,
            CP,
        ));
    }
    out
}

/// A raw margin: value and derivative (both from the same arithmetic as
/// the construction), or undefined.
type Raw<T> = Option<T>;

/// The scalar construction's raw margins at `p`, with the construction's
/// own expressions and evaluation order, and each prerequisite tested
/// before the operation it protects.
fn scalar_raw<T: Real>(p: &[T; N_PARAMS]) -> Vec<Raw<T>> {
    let c = T::cst;
    let half = c(0.5);
    let [mag, mh, mw, sw1, sw2, sw3, sw4, sd1, sr1] = *p;
    let alpha = HALF_SLOT;
    let (sa, ca) = (alpha.sin(), alpha.cos());
    // Rotor.
    let x_in = c(R_ROTOR) - mag - mh;
    let m_shaft = x_in - c(R_SHAFT);
    let w = mw * half;
    // Opening (evaluated before the arcsine).
    let s = sw2 * half;
    let wt = sw1 * half;
    let m_open_hi = c(R_BORE * sa) - s;
    let opening_ok = s.value() > 0.0 && s.value() < R_BORE * sa;
    // Flank and fillet quantities that need no arcsine or root.
    let x_b = sd1 * half - sw4;
    let x_f = x_b - sr1;
    let y_f = (x_f * c(sa) - wt - sr1) / c(ca);
    let f_hi0 = x_f - sr1 * c(sa);
    let (mut disc_m, mut arc_c, mut arc_r, mut flank) = (None, None, None, None);
    if opening_ok {
        let phi_o = (s / c(R_BORE)).asin();
        let o_hi0 = c(R_BORE) * phi_o.cos();
        let x_c = o_hi0 + sw3;
        let qa = c(ca * ca);
        let qb = x_c - wt * c(sa);
        let qc = x_c * x_c + s * s - wt * wt;
        let disc = qb * qb - qa * qc;
        disc_m = Some(disc);
        if !(disc.value().is_nan() || disc.value() < 0.0) {
            let u = (qb + disc.sqrt()) / qa;
            let rho = u * c(sa) - wt;
            arc_c = Some(u - x_c);
            arc_r = Some(rho - s);
            let t_hi0 = u - rho * c(sa);
            flank = Some(f_hi0 - t_hi0);
        }
    }
    vec![
        Some(m_shaft),
        Some(mh),
        Some(w),
        Some(s),
        Some(m_open_hi),
        disc_m,
        arc_c,
        arc_r,
        Some(sr1),
        Some(y_f),
        flank,
    ]
}

/// The coupling-circle margins of the 55 control points. The stator ones
/// need the arcsine of the opening, so they are undefined (`None`) unless
/// both opening guards pass; the rotor points do not depend on `SW2`.
/// Values are `hypot` of the values, exactly as the construction tests.
fn cp_raw<const N: usize>(p: &[DualN<N>; N_CP_PARAMS], opening_ok: bool) -> Vec<Raw<DualN<N>>> {
    let k_sw2 = IcemDesign::index_of("SW2").expect("SW2");
    let mut q = *p;
    if !opening_ok {
        // A safe SW2 for the rotor points, which do not depend on it.
        q[k_sw2] = DualN::cst(SW2_GENERATOR_DEFAULT);
    }
    let pts = control_point_positions(&q);
    pts.iter()
        .zip(control_points())
        .enumerate()
        .map(|(i, (pt, cp))| {
            if i >= N_ROTOR_POINTS && !opening_ok {
                return None;
            }
            let r_val = pt[0].value().hypot(pt[1].value());
            let r = (pt[0] * pt[0] + pt[1] * pt[1]).sqrt();
            let m = match cp.part {
                CpPart::Rotor => DualN::cst(R_COUPLING) - r,
                CpPart::Stator => r - DualN::cst(R_COUPLING),
            };
            let v = match cp.part {
                CpPart::Rotor => R_COUPLING - r_val,
                CpPart::Stator => r_val - R_COUPLING,
            };
            Some(DualN { v, d: m.d })
        })
        .collect()
}

fn passes(spec: &GuardSpec, m: Option<f64>) -> bool {
    match m {
        None => false,
        Some(v) if spec.strict => v > 0.0,
        Some(v) => v >= 0.0,
    }
}

/// Assemble the evaluations from raw margins, scales and the derivative
/// domain flag (module docs).
fn assemble<const N: usize>(
    specs: Vec<GuardSpec>,
    raw: Vec<Raw<DualN<N>>>,
    scales: &[f64],
    disc_zero: bool,
) -> Vec<GuardEval> {
    specs
        .into_iter()
        .zip(raw)
        .zip(scales)
        .map(|((spec, m), &scale)| {
            let margin = m.map(|d| d.v);
            let pass = passes(&spec, margin);
            let (c, grad_h) = match m {
                Some(d) => {
                    let singular = disc_zero && spec.requires.contains(&"slot_top_discriminant");
                    let g: Vec<f64> = d.d.iter().map(|x| x / scale).collect();
                    let grad = (!singular && g.iter().all(|x| x.is_finite())).then_some(g);
                    (d.v / scale, grad)
                }
                None => (-1.0, Some(vec![0.0; N])),
            };
            GuardEval {
                spec,
                margin,
                scale,
                c,
                h: c - TAU,
                grad_h,
                pass,
            }
        })
        .collect()
}

fn discrete(values: &[f64]) -> Vec<DiscreteEval> {
    vec![DiscreteEval {
        name: "components_finite",
        pass: values.iter().all(|v| v.is_finite()),
        source: "sector_curves discriminant is_nan test; FrozenSpace::embed",
    }]
}

/// The scale of each guard: `|m_i(θ0)|`, or the stated zero-reference
/// value.
fn scales_of(specs: &[GuardSpec], at_theta0: &[Option<f64>]) -> Vec<f64> {
    specs
        .iter()
        .zip(at_theta0)
        .map(|(s, m)| {
            let m0 = m.unwrap_or_else(|| panic!("guard {} undefined at θ0", s.name));
            if m0 != 0.0 {
                return m0.abs();
            }
            ZERO_REFERENCE_SCALES
                .iter()
                .find(|(n, _, _)| *n == s.name)
                .map(|(_, v, _)| *v)
                .unwrap_or_else(|| panic!("guard {} has a zero θ0 margin and no scale", s.name))
        })
        .collect()
}

fn opening_ok(values: &[f64]) -> bool {
    let s = values[IcemDesign::index_of("SW2").expect("SW2")] * 0.5;
    s > 0.0 && s < R_BORE * HALF_SLOT.sin()
}

fn disc_is_zero<T: Real>(raw: &[Raw<T>]) -> bool {
    raw[5].map(|d| d.value() == 0.0).unwrap_or(false)
}

/// The raw margins of the 9-parameter construction at `values` (no
/// derivatives), for the scales.
fn scalar_margins_f64(values: &[f64; N_PARAMS]) -> Vec<Option<f64>> {
    scalar_raw(values)
}

/// The θ0 raw margins of the scalar guards.
pub fn scalar_theta0_margins() -> Vec<Option<f64>> {
    scalar_margins_f64(&IcemDesign::INITIAL.values)
}

/// The θ0 raw margins of the control-point guards.
pub fn cp_theta0_margins() -> Vec<Option<f64>> {
    let v = IcemCpDesign::INITIAL.values;
    let p: [DualN<N_CP_PARAMS>; N_CP_PARAMS] = std::array::from_fn(|k| DualN::cst(v[k]));
    let scalars: [DualN<N_CP_PARAMS>; N_PARAMS] = std::array::from_fn(|k| p[k]);
    scalar_raw(&scalars)
        .into_iter()
        .chain(cp_raw(&p, opening_ok(&v)))
        .map(|m| m.map(|d| d.v))
        .collect()
}

/// The geometry domain of the 9-parameter construction at `design`.
pub fn scalar_domain(design: &IcemDesign) -> Domain {
    let specs = scalar_guard_specs();
    let scales = scales_of(&specs, &scalar_theta0_margins());
    let v = design.values;
    let disc = discrete(&v);
    let p: [DualN<N_PARAMS>; N_PARAMS] = std::array::from_fn(|k| DualN::var(v[k], k));
    let raw = if disc.iter().all(|d| d.pass) {
        scalar_raw(&p)
    } else {
        vec![None; specs.len()]
    };
    let disc_zero = disc_is_zero(&raw);
    let construction = if disc.iter().all(|d| d.pass) {
        sector_curves(&v).map(|_| ()).map_err(|e| e.to_string())
    } else {
        Err("a component is not finite".into())
    };
    Domain {
        components: PARAM_NAMES.to_vec(),
        guards: assemble(specs, raw, &scales, disc_zero),
        discrete: disc,
        construction,
    }
}

/// The geometry domain of the control-point construction at `design`.
pub fn cp_domain(design: &IcemCpDesign) -> Domain {
    let specs = cp_guard_specs();
    let scales = scales_of(&specs, &cp_theta0_margins());
    let v = design.values;
    let disc = discrete(&v);
    let p: [DualN<N_CP_PARAMS>; N_CP_PARAMS] = std::array::from_fn(|k| DualN::var(v[k], k));
    let raw: Vec<Raw<DualN<N_CP_PARAMS>>> = if disc.iter().all(|d| d.pass) {
        let scalars: [DualN<N_CP_PARAMS>; N_PARAMS] = std::array::from_fn(|k| p[k]);
        scalar_raw(&scalars)
            .into_iter()
            .chain(cp_raw(&p, opening_ok(&v)))
            .collect()
    } else {
        vec![None; specs.len()]
    };
    let disc_zero = disc_is_zero(&raw);
    let construction = if disc.iter().all(|d| d.pass) {
        sector_curves_cp(&v).map(|_| ()).map_err(|e| e.to_string())
    } else {
        Err("a component is not finite".into())
    };
    Domain {
        components: cp_param_names().to_vec(),
        guards: assemble(specs, raw, &scales, disc_zero),
        discrete: disc,
        construction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theta0_margins_are_all_nonzero_and_theta0_is_admissible() {
        for m in scalar_theta0_margins()
            .into_iter()
            .chain(cp_theta0_margins())
        {
            assert!(m.is_some_and(|v| v != 0.0));
        }
        assert!(ZERO_REFERENCE_SCALES.is_empty());
        let a = scalar_domain(&IcemDesign::INITIAL);
        let b = cp_domain(&IcemCpDesign::INITIAL);
        for d in [&a, &b] {
            assert!(d.buildable() && d.admissible() && d.jacobian_defined());
            // At θ0 every scaled margin is exactly 1.
            for g in &d.guards {
                assert_eq!(g.c, 1.0, "{}", g.spec.name);
            }
        }
        assert_eq!(a.guards.len(), 11);
        assert_eq!(b.guards.len(), 11 + 55);
    }

    #[test]
    fn exact_zero_on_a_strict_guard_is_not_buildable() {
        // Sr1 = 0 and MH = 0 exactly: the strict guards fail at m = 0, as
        // the construction does; no epsilon.
        for (name, k) in [("fillet_radius", 8), ("magnet_height", 1)] {
            let mut d = IcemDesign::INITIAL;
            d.values[k] = 0.0;
            let dom = scalar_domain(&d);
            let g = dom.guards.iter().find(|g| g.spec.name == name).unwrap();
            assert_eq!(g.margin, Some(0.0));
            assert!(!g.pass && !dom.buildable(), "{name}");
            assert!(sector_curves(&d.values).is_err());
            // And the next float up passes that guard.
            d.values[k] = f64::from_bits(1);
            let up = scalar_domain(&d);
            assert!(up.guards.iter().find(|g| g.spec.name == name).unwrap().pass);
        }
    }

    #[test]
    fn undefined_downstream_margins_get_the_safe_value() {
        // SW2 beyond the pitch: the arcsine is never taken; the arc and
        // flank margins are undefined, with c = −1 and a zero derivative.
        let mut d = IcemDesign::INITIAL;
        d.values[4] = 2.0 * R_BORE * HALF_SLOT.sin();
        let dom = scalar_domain(&d);
        assert!(!dom.buildable());
        for g in &dom.guards {
            assert!(g.h.is_finite() && g.c.is_finite(), "{}", g.spec.name);
            if g.spec.requires.contains(&"opening_vs_pitch") {
                assert_eq!(g.margin, None);
                assert_eq!(g.c, -1.0);
                assert!(!g.pass);
                assert!(g.grad_h.as_ref().unwrap().iter().all(|x| *x == 0.0));
            }
        }
        // A non-finite component: every guard undefined, the discrete
        // guard fails, nothing is NaN.
        let mut d = IcemDesign::INITIAL;
        d.values[0] = f64::NAN;
        let dom = scalar_domain(&d);
        assert!(!dom.discrete_pass() && !dom.buildable());
        assert!(dom.guards.iter().all(|g| g.h == -1.0 - TAU));
    }
}
