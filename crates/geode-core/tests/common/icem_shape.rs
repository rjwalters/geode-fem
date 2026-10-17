//! Shared set-up for the **shape adjoint** of the ICEM sector torque (motor
//! paper M3b, issue #1174): the [`SectorShapeProblem`] built from an
//! [`IcemModel`](super::icem::IcemModel), the operating points of the
//! reference's optimization grid, smooth test velocities, and the Taylor /
//! finite-difference drivers.
//!
//! Used by `tests/motor_shape_gradient.rs` (CI-fast checks) and
//! `examples/icem_shape_gradient.rs` (the full study in
//! `benchmarks/motor_shape_gradient/`). The including crate root must
//! declare `mod icem;` (the `icem_forward.rs` model) next to this module.
#![allow(dead_code)] // each binary uses a subset

use std::f64::consts::PI;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::assembly::magnetostatic_sector::SectorError;
use geode_core::assembly::magnetostatic_shape::{
    AngleSetGradient, ArkkioTerm, OperatingPoint, SectorShapeProblem, ShapeSolve, ShapeTorque,
    directional,
};

use crate::icem::{
    self, IcemModel, L_AXIAL, R_COUPLING, R_ROTOR, R_STATOR, TAG_GAP_INNER, TAG_GAP_OUTER,
    TAG_WINDING0, TURNS, WINDING_GROUPS,
};

/// The reference's optimization angle grid, `0, 2, ..., 18` mechanical
/// degrees (`conventions.json` `torque.optimization_grid_mechanical_degrees`).
pub const OPT_DEGREES: [i64; 10] = [0, 2, 4, 6, 8, 10, 12, 14, 16, 18];

/// The shape problem of an ICEM model: winding group `g` (tag `10 + g`)
/// is group `g − 1`, with `J = I·N·sign / A_g(X)` (the `AreaMode::Mesh`
/// rule, areas moving with the mesh).
pub fn shape_problem(m: &IcemModel) -> Result<SectorShapeProblem, SectorError> {
    let groups = m
        .band
        .tri_tags()
        .iter()
        .map(|&t| {
            let g = t - TAG_WINDING0;
            (1..=12).contains(&g).then(|| (g - 1) as usize)
        })
        .collect();
    SectorShapeProblem::new(m.band.clone(), m.materials.clone(), groups, &m.dirichlet)?
        .with_magnetization(&m.magnetization)?
        .with_coupling(m.coupling.clone())
}

/// Operating points at integer mechanical degrees: group currents
/// `I_q(β) · N · sign` (`excitation.current_convention`).
pub fn operating_points(
    m: &IcemModel,
    i_peak: f64,
    phi0: f64,
    degrees: &[i64],
) -> Vec<OperatingPoint> {
    degrees
        .iter()
        .map(|&deg| {
            let beta = deg as f64 * PI / 180.0;
            let group_currents = WINDING_GROUPS
                .iter()
                .map(|&(ph, sign)| icem::phase_current(i_peak, phi0, beta, ph) * TURNS * sign)
                .collect();
            OperatingPoint {
                k: deg * m.steps_per_degree(),
                group_currents,
            }
        })
        .collect()
}

/// Arkkio over both gap bands, normalised by 44.0-45.0 mm (`IcemModel::arkkio(_, 2)`).
pub fn arkkio_spec() -> ShapeTorque {
    ShapeTorque::Arkkio {
        terms: vec![
            ArkkioTerm {
                tag: TAG_GAP_INNER,
                r_inner: R_ROTOR,
                r_outer: R_STATOR,
            },
            ArkkioTerm {
                tag: TAG_GAP_OUTER,
                r_inner: R_ROTOR,
                r_outer: R_STATOR,
            },
        ],
        l_axial: L_AXIAL,
    }
}

/// The harmonic coupling torque (N m, axial length included).
pub fn coupling_spec() -> ShapeTorque {
    ShapeTorque::Coupling { l_axial: L_AXIAL }
}

/// Where a test velocity is allowed to be non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Rotor and stator only: zero on every node of the gap-band triangles
    /// (tags 4 and 5) and on the Dirichlet circles.
    OffBand,
    /// Also on the band nodes off the coupling circle (the rotor surface at
    /// 44.0 mm and the stator bore at 45.0 mm): exercises the explicit
    /// Arkkio terms. Zero on the coupling circle and the Dirichlet circles.
    WithBand,
}

/// A smooth, sector-equivariant node velocity (max |V| = 1, so `h` is a
/// length in metres): `V = v_r e_r + v_θ e_θ` with `v_r, v_θ` 60°-periodic
/// in θ (so the radial-edge pairs stay exact rotations of each other) and a
/// radial cut-off that vanishes on the Dirichlet circles and on the gap
/// band (or only on the coupling circle, [`Support::WithBand`]). `seed`
/// picks the mode amplitudes and phases.
pub fn smooth_velocity(m: &IcemModel, seed: u64, support: Support) -> Vec<[f64; 2]> {
    let nodes = &m.band.mesh().nodes;
    let n = nodes.len();
    let dir = m.band.expand_nodal(&m.dirichlet);
    let radius = |p: [f64; 2]| p[0].hypot(p[1]);
    let r_in = (0..n)
        .filter(|&i| dir[i])
        .map(|i| radius(nodes[i]))
        .fold(f64::INFINITY, f64::min);
    let r_out = (0..n)
        .filter(|&i| dir[i])
        .map(|i| radius(nodes[i]))
        .fold(0.0, f64::max);
    let mut zero = dir.clone();
    for c in m.band.coupling_nodes() {
        zero[c as usize] = true;
    }
    if support == Support::OffBand {
        for (tri, &t) in m.band.mesh().tris.iter().zip(m.band.tri_tags()) {
            if t == TAG_GAP_INNER || t == TAG_GAP_OUTER {
                for &p in tri {
                    zero[p as usize] = true;
                }
            }
        }
    }
    let (r_lo, r_hi) = match support {
        Support::OffBand => (R_ROTOR, R_STATOR),
        Support::WithBand => (R_COUPLING, R_COUPLING),
    };
    // Deterministic pseudo-random coefficients (SplitMix64).
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    let mut rnd = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let modes: Vec<[f64; 6]> = (0..4)
        .map(|_| [rnd(), rnd(), rnd(), rnd(), rnd(), rnd()])
        .collect();
    let mut v = vec![[0.0; 2]; n];
    for i in 0..n {
        if zero[i] {
            continue;
        }
        let p = nodes[i];
        let r = radius(p);
        let th = p[1].atan2(p[0]);
        let s = if r <= r_lo {
            (r - r_in) / (r_lo - r_in)
        } else if r >= r_hi {
            (r - r_hi) / (r_out - r_hi)
        } else {
            continue;
        };
        if !(0.0..=1.0).contains(&s) {
            continue;
        }
        let chi = 4.0 * s * (1.0 - s);
        let (mut vr, mut vt) = (0.0, 0.0);
        for (mi, c) in modes.iter().enumerate() {
            let mm = (mi + 1) as f64;
            let ang = 6.0 * mm * th;
            let rad = PI * mm * s;
            vr += c[0] * (ang + 3.0 * c[1]).cos() * (rad + 3.0 * c[2]).cos() / mm;
            vt += c[3] * (ang + 3.0 * c[4]).sin() * (rad + 3.0 * c[5]).cos() / mm;
        }
        let (cs, sn) = (th.cos(), th.sin());
        v[i] = [chi * (vr * cs - vt * sn), chi * (vr * sn + vt * cs)];
    }
    let vmax = v.iter().map(|d| d[0].hypot(d[1])).fold(0.0, f64::max);
    for d in v.iter_mut() {
        d[0] /= vmax;
        d[1] /= vmax;
    }
    v
}

/// The two objectives of an angle set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Objective {
    Mean,
    Std,
}

impl Objective {
    pub fn name(self) -> &'static str {
        match self {
            Objective::Mean => "mean",
            Objective::Std => "std",
        }
    }

    pub fn value(self, t: &[f64]) -> f64 {
        let n = t.len() as f64;
        let mean = t.iter().sum::<f64>() / n;
        match self {
            Objective::Mean => mean,
            Objective::Std => (t.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n).sqrt(),
        }
    }

    pub fn grad(self, g: &AngleSetGradient) -> &[[f64; 2]] {
        match self {
            Objective::Mean => &g.mean_grad,
            Objective::Std => &g.std_grad,
        }
    }
}

/// The base design: solves at every operating point and the gradients.
pub struct Base {
    pub solves: Vec<ShapeSolve>,
    pub grad: AngleSetGradient,
}

impl Base {
    pub fn new(
        p: &SectorShapeProblem,
        ops: &[OperatingPoint],
        torque: &ShapeTorque,
        cfg: &NewtonConfig,
    ) -> Result<Self, SectorError> {
        let solves = p.solve_angle_set(ops, torque, cfg, None)?;
        let grad = p.angle_set_gradient(&solves, torque)?;
        Ok(Self { solves, grad })
    }

    pub fn initials(&self) -> Vec<Vec<f64>> {
        self.solves
            .iter()
            .map(|s| s.solution.solution.a_z.clone())
            .collect()
    }
}

/// Torques of the design `X + h V`, warm-started from the base solves.
pub fn perturbed_torques(
    p: &SectorShapeProblem,
    base: &Base,
    ops: &[OperatingPoint],
    torque: &ShapeTorque,
    cfg: &NewtonConfig,
    v: &[[f64; 2]],
    h: f64,
) -> Result<Vec<f64>, SectorError> {
    let q = p.displaced(v, h)?;
    let init = base.initials();
    Ok(q.solve_angle_set(ops, torque, cfg, Some(&init))?
        .iter()
        .map(|s| s.torque)
        .collect())
}

/// One Taylor row: `h`, `J(X + hV) − J(X)`, `h ⟨dJ, V⟩`, and the remainder.
#[derive(Debug, Clone, Copy)]
pub struct TaylorRow {
    pub h: f64,
    pub delta: f64,
    pub linear: f64,
    pub remainder: f64,
}

/// Taylor rows for both objectives at the given steps (one forward sweep
/// per step serves both).
pub fn taylor(
    p: &SectorShapeProblem,
    base: &Base,
    ops: &[OperatingPoint],
    torque: &ShapeTorque,
    cfg: &NewtonConfig,
    v: &[[f64; 2]],
    hs: &[f64],
) -> Result<[Vec<TaylorRow>; 2], SectorError> {
    let objs = [Objective::Mean, Objective::Std];
    let j0 = objs.map(|o| o.value(&base.grad.torques));
    let dj = objs.map(|o| directional(o.grad(&base.grad), v));
    let mut out = [Vec::new(), Vec::new()];
    for &h in hs {
        let t = perturbed_torques(p, base, ops, torque, cfg, v, h)?;
        for (oi, o) in objs.iter().enumerate() {
            let delta = o.value(&t) - j0[oi];
            let linear = h * dj[oi];
            out[oi].push(TaylorRow {
                h,
                delta,
                linear,
                remainder: (delta - linear).abs(),
            });
        }
    }
    Ok(out)
}

/// Observed orders `log(r_i / r_{i+1}) / log(h_i / h_{i+1})`.
pub fn observed_orders(rows: &[TaylorRow]) -> Vec<f64> {
    rows.windows(2)
        .map(|w| (w[0].remainder / w[1].remainder).ln() / (w[0].h / w[1].h).ln())
        .collect()
}

/// Central differences `(J(X+hV) − J(X−hV)) / 2h` for both objectives.
pub fn central_differences(
    p: &SectorShapeProblem,
    base: &Base,
    ops: &[OperatingPoint],
    torque: &ShapeTorque,
    cfg: &NewtonConfig,
    v: &[[f64; 2]],
    hs: &[f64],
) -> Result<[Vec<(f64, f64)>; 2], SectorError> {
    let objs = [Objective::Mean, Objective::Std];
    let mut out = [Vec::new(), Vec::new()];
    for &h in hs {
        let tp = perturbed_torques(p, base, ops, torque, cfg, v, h)?;
        let tm = perturbed_torques(p, base, ops, torque, cfg, v, -h)?;
        for (oi, o) in objs.iter().enumerate() {
            out[oi].push((h, (o.value(&tp) - o.value(&tm)) / (2.0 * h)));
        }
    }
    Ok(out)
}

/// The steps of `hs` for which both `X + hV` and `X − hV` are valid meshes
/// (no inverted triangle), and the rejected ones.
pub fn valid_steps(p: &SectorShapeProblem, v: &[[f64; 2]], hs: &[f64]) -> (Vec<f64>, Vec<f64>) {
    hs.iter()
        .partition(|&&h| p.displaced(v, h).is_ok() && p.displaced(v, -h).is_ok())
}

/// Objective noise floor: the mean and std of the base design re-solved
/// from a cold start at every angle, minus the warm-started base values.
pub fn noise_floor(
    p: &SectorShapeProblem,
    base: &Base,
    ops: &[OperatingPoint],
    torque: &ShapeTorque,
    cfg: &NewtonConfig,
) -> Result<[f64; 3], SectorError> {
    let mut t = Vec::with_capacity(ops.len());
    for op in ops {
        t.push(p.solve(op, torque, cfg, None)?.torque);
    }
    let max_dt = t
        .iter()
        .zip(&base.grad.torques)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    Ok([
        max_dt,
        (Objective::Mean.value(&t) - base.grad.mean).abs(),
        (Objective::Std.value(&t) - base.grad.std).abs(),
    ])
}
