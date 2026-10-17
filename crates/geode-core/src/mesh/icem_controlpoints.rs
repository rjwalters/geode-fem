//! **ICEM reference motor: free-form control-point design space** (motor
//! paper M4 prep, issue #1179).
//!
//! The reference optimizer's design vector is the 12 scalars of
//! [`super::icem_parametric`] (9 geometric) **plus** 10 free-form offsets:
//! 7 rotor groups (18 control points) and 3 stator groups (37 control
//! points) of the NURBS multipatch geometry (`PMSM_optimization_nonlin.m`
//! lines 114–124 and `MotorOptimizationFull.m` lines 58–59 of the pinned
//! upstream snapshot, read as data). This module adds the 10 offsets as
//! extra components of `θ`, with `X(θ)` and `∂X/∂θ` on the fixed topology of
//! a mesh generated once at `θ0`. It is written for GEODE; no upstream code
//! is translated.
//!
//! # The upstream parametrization (as read from the data)
//!
//! * **Offset direction.** Each control point `P_i` of group `g` moves by
//!   `δ_g u_i`, where `u_i = (cos a_i, sin a_i)` and `a_i = atan2(P_i)` is the
//!   polar angle of the point in the geometry built **once at set-up**, with
//!   the generators' function defaults. The direction is frozen: it does not
//!   follow the scalars. Only the Cartesian control point moves; its weight
//!   is the generator's weight at the current scalars. For the rotor the
//!   set-up angles are those of every design (no scalar moves the rotor
//!   surface). For the stator they are those at the generator default
//!   `SW2 = 2.3 mm` ([`SW2_GENERATOR_DEFAULT`]), which is also the
//!   registered initial value: at another `SW2` a slot-opening corner moves
//!   along its `2.3 mm` direction, not along its current radius.
//! * **Ties.** One offset per group, shared by all its points. A control
//!   point on a patch interface is one global point, so every patch that
//!   uses it moves with it.
//! * **Rotor groups** (`CPR1`..`CPR7`, offsets in `[−1.5, 0]` mm, `CPR3` in
//!   `[−1, 0]` mm). The 18 points are the control points of the rotor
//!   surface, which the export represents in five pieces (lab frame,
//!   ascending angle): the sector-edge arc `60°..68.48°` (3 points), the
//!   bridge arc `68.48°..68.80°` (3), the pole arc `68.80°..111.20°`
//!   (degree 2 with 7 interior knots, 10 points), and the mirror images of
//!   the first two. Shared end points counted once, that is
//!   `3 + 3 + 10 + 3 + 3 − 4 = 18`. Numbering them `0..17` by angle:
//!   - `CPR1` = {0, 17}: the surface points on the sector edges (upstream
//!     global numbers 43, 83);
//!   - `CPR2` = {1, 16}: the middle points of the edge arcs (44, 84);
//!   - `CPR3` = {2, 3, 4, 13, 14, 15}: the three points of each bridge arc
//!     (40, 47, 48, 79, 80, 50);
//!   - `CPR4`..`CPR7` = {5, 12}, {6, 11}, {7, 10}, {8, 9}: the interior points
//!     of the pole arc in mirror pairs, from the bridges to the pole axis
//!     (59/66, 60/65, 61/64, 62/63).
//!
//!   Every group is symmetric about the pole axis, and `CPR1` ties the two
//!   antiperiodic edge points, so the sector edges stay `R(60°)` images.
//! * **Stator groups** (`CPS1`..`CPS3`, offsets in `[0, 0.5]` mm). The 37
//!   points are all control points of the bore: per slot pitch, three
//!   rational quadratic arcs (tooth centre to opening corner, the opening,
//!   opening corner to tooth centre), that is 6 new points per slot and the
//!   last tooth centre:
//!   - `CPS1` (18): the two opening corners and the opening arc's middle
//!     point of each slot;
//!   - `CPS2` (12): the middle points of the two tooth arcs of each slot;
//!   - `CPS3` (7): the tooth-centre bore points at `60°, 70°, …, 120°`,
//!     including both antiperiodic edge points.
//!
//! The upstream global point numbers come from the GeoPDEs multipatch
//! numbering, which is not part of the pinned snapshot, so they were not
//! reproduced here. The groups above are identified from the data instead:
//! the group sizes (2, 2, 6, 2, 2, 2, 2 and 18, 12, 7) and bounds, the
//! mirror symmetry, and the iteration-120 export, whose control points
//! differ from the scalar-only construction by exactly one offset per group
//! along the frozen directions, to round-off
//! (`benchmarks/motor_reference/parametric_cp/`). The alternative stator
//! assignment (arc middles with the opening) leaves a 3e-8 m misfit.
//! Within the rotor, the order `CPR4..CPR7` and `CPR1` against `CPR2`
//! follow the upstream numbers (contiguous `59..66` along the pole row,
//! `43, 44` along the edge row); the fit itself does not depend on which
//! label each pair carries.
//!
//! # Curves and node motion
//!
//! [`sector_curves_cp`] is [`super::icem_parametric::sector_curves`] with
//! these replacements (names in parentheses):
//!
//! * the rotor surface arc becomes the five rational B-spline pieces
//!   (`rotor.surface_edge_lo`, `rotor.surface_bridge_lo`,
//!   `rotor.surface_pole`, `rotor.surface_bridge_hi`,
//!   `rotor.surface_edge_hi`) through the moved control points;
//! * each rotor radial edge is split at the surface into an iron part from
//!   the shaft (`rotor.edge_lo_iron`) and a gap part to the coupling circle
//!   (`rotor.edge_lo_gap`), meeting at the moved edge point;
//! * each slot's three bore arcs become rational quadratic splines through
//!   the moved bore points (same names), the opening-channel walls start at
//!   the moved corners, and the stator radial edges meet the bore at the
//!   moved tooth-centre points.
//!
//! A spline node keeps its knot parameter (on a moving arc the angle
//! fraction of [`super::icem_parametric`] is not defined). So even at zero
//! offsets, `SW2` moves the bore nodes by a different rule than the
//! 9-parameter map (both keep them on the bore circle). The interior
//! extension and its options are those of
//! [`super::icem_parametric::IcemShapeMap`]: [`EdgeRule::Slide`] applies to
//! the stator radial edges as there, and the split rotor edges keep their
//! nodes' length fraction (a prescribed radial stretch, which `CPR1` makes
//! nonzero). The default is the plain harmonic extension
//! ([`CP_MOTION_DEFAULT`], `χ = 0`), not the Jacobian-stiffened one of the
//! 9-parameter map.
//!
//! **Air gap.** The coupling circles (tags 121, 122) never move, for any
//! `θ`. The inner gap band (tag 4) lies between the rotor surface and the
//! coupling circle, so the 7 rotor groups deform it; no other component
//! moves it. The outer band (tag 5) lies between the coupling circle and
//! the bore, so `SW2` and the 3 stator groups deform it; no other component
//! moves it. This replaces the #1176 rule that tag 4 never moves: that rule
//! holds for the 12 scalars only, because a free-form rotor surface is the
//! band's inner boundary.

use super::icem_parametric::{
    AttachmentReport, Curve, CurveShape, DualN, EdgeRule, HALF_POLE, HALF_SLOT, IcemDesign,
    IcemParamError, MotionCore, MotionOptions, N_PARAMS, NodeAttachment, PARAM_NAMES,
    POCKET_ANGLE_INNER, POCKET_ANGLE_OUTER, R_BORE, R_COUPLING, R_ROTOR, ROTOR_FRAME, Real,
    SLOTS_IN_SECTOR, STATOR_FRAME0, line, mirror, polar, rot, sector_curves,
};
use super::tagged_tri::TaggedTriMesh;
use crate::analytic::waveguide::TriMesh;

/// Rotor control-point groups.
pub const N_ROTOR_GROUPS: usize = 7;
/// Stator control-point groups.
pub const N_STATOR_GROUPS: usize = 3;
/// Free-form offsets: rotor groups then stator groups.
pub const N_OFFSETS: usize = N_ROTOR_GROUPS + N_STATOR_GROUPS;
/// Geometric design components: [`PARAM_NAMES`] then [`OFFSET_NAMES`].
pub const N_CP_PARAMS: usize = N_PARAMS + N_OFFSETS;

/// Offset names, in the upstream registration order (rotor groups, then
/// stator groups).
pub const OFFSET_NAMES: [&str; N_OFFSETS] = [
    "CPR1", "CPR2", "CPR3", "CPR4", "CPR5", "CPR6", "CPR7", "CPS1", "CPS2", "CPS3",
];

/// Box of each offset (m), `(min, max)`, from `PMSM_optimization_nonlin.m`
/// lines 114–124. The upstream initial value is 0 for every group, which is
/// the upper bound of the rotor boxes and the lower bound of the stator
/// boxes.
pub const OFFSET_BOUNDS: [(f64, f64); N_OFFSETS] = [
    (-1.5e-3, 0.0),
    (-1.5e-3, 0.0),
    (-1.0e-3, 0.0),
    (-1.5e-3, 0.0),
    (-1.5e-3, 0.0),
    (-1.5e-3, 0.0),
    (-1.5e-3, 0.0),
    (0.0, 0.5e-3),
    (0.0, 0.5e-3),
    (0.0, 0.5e-3),
];

/// Upstream global control-point numbers of each rotor group (data from
/// `PMSM_optimization_nonlin.m` lines 114–120).
pub const UPSTREAM_ROTOR_POINTS: [&[usize]; N_ROTOR_GROUPS] = [
    &[43, 83],
    &[44, 84],
    &[40, 47, 48, 79, 80, 50],
    &[59, 66],
    &[60, 65],
    &[61, 64],
    &[62, 63],
];

/// Upstream global control-point numbers of each stator group (data from
/// `PMSM_optimization_nonlin.m` lines 122–124).
pub const UPSTREAM_STATOR_POINTS: [&[usize]; N_STATOR_GROUPS] = [
    &[
        194, 198, 199, 273, 276, 277, 348, 351, 352, 423, 426, 427, 498, 501, 502, 573, 576, 577,
    ],
    &[196, 202, 274, 280, 349, 355, 424, 430, 499, 505, 574, 578],
    &[195, 201, 279, 354, 429, 504, 579],
];

/// The stator generator's default slot-opening width (m), at which the
/// upstream set-up computes the stator offset directions.
pub const SW2_GENERATOR_DEFAULT: f64 = 0.0023;

/// Interior knots of the pole arc (the rotor generator's `REFINEMENT`).
pub const POLE_ARC_INTERIOR_KNOTS: usize = 7;

/// Rotor surface control points.
pub const N_ROTOR_POINTS: usize = 18;
/// Stator bore control points.
pub const N_STATOR_POINTS: usize = 6 * SLOTS_IN_SECTOR + 1;

/// [`DualN`] over the 19 geometric components ([`cp_param_names`] order).
pub type CpDual = DualN<N_CP_PARAMS>;

/// Names of the 19 geometric components: [`PARAM_NAMES`] then
/// [`OFFSET_NAMES`].
pub fn cp_param_names() -> [&'static str; N_CP_PARAMS] {
    let mut out = [""; N_CP_PARAMS];
    out[..N_PARAMS].copy_from_slice(&PARAM_NAMES);
    out[N_PARAMS..].copy_from_slice(&OFFSET_NAMES);
    out
}

/// A point of the 19-component geometric design space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IcemCpDesign {
    /// The 9 scalars ([`PARAM_NAMES`] order, m), then the 10 offsets
    /// ([`OFFSET_NAMES`] order, m).
    pub values: [f64; N_CP_PARAMS],
}

impl IcemCpDesign {
    /// The optimizer's initial design: [`IcemDesign::INITIAL`] and zero
    /// offsets.
    pub const INITIAL: IcemCpDesign = IcemCpDesign {
        values: [
            0.007, 0.007, 0.019, 0.004, 0.0023, 0.001, 0.00825, 0.135, 0.001, 0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ],
    };

    /// The design with scalars `scalars` and offsets `offsets`.
    pub fn from_parts(scalars: &IcemDesign, offsets: &[f64; N_OFFSETS]) -> Self {
        let mut values = [0.0; N_CP_PARAMS];
        values[..N_PARAMS].copy_from_slice(&scalars.values);
        values[N_PARAMS..].copy_from_slice(offsets);
        Self { values }
    }

    /// The 9 scalars.
    pub fn scalars(&self) -> IcemDesign {
        let mut values = [0.0; N_PARAMS];
        values.copy_from_slice(&self.values[..N_PARAMS]);
        IcemDesign { values }
    }

    /// The 10 offsets.
    pub fn offsets(&self) -> [f64; N_OFFSETS] {
        let mut o = [0.0; N_OFFSETS];
        o.copy_from_slice(&self.values[N_PARAMS..]);
        o
    }

    /// Index of component `name` in [`cp_param_names`].
    pub fn index_of(name: &str) -> Option<usize> {
        cp_param_names().iter().position(|n| *n == name)
    }
}

/// Which part a control point belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpPart {
    /// Rotor surface.
    Rotor,
    /// Stator bore.
    Stator,
}

/// One free-form control point.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlPoint {
    /// Rotor or stator.
    pub part: CpPart,
    /// Stable label, e.g. `rotor.p05` or `slot2.corner_lo`, `tooth3`.
    pub label: String,
    /// Offset index (`0..N_OFFSETS`, [`OFFSET_NAMES`] order).
    pub group: usize,
    /// Frozen unit offset direction `u_i` (lab frame).
    pub direction: [f64; 2],
}

/// Homogeneous knot insertion (Boehm) of `u` into a degree-`p` rational
/// curve with homogeneous control points `pw = (w x, w y, w)`.
fn insert_knot(p: usize, knots: &[f64], pw: &[[f64; 3]], u: f64) -> (Vec<f64>, Vec<[f64; 3]>) {
    let n = pw.len();
    // Span k with knots[k] <= u < knots[k + 1].
    let mut k = p;
    while k + 1 < n && u >= knots[k + 1] {
        k += 1;
    }
    let mut q = Vec::with_capacity(n + 1);
    for i in 0..=n {
        if i + p <= k {
            q.push(pw[i]);
        } else if i > k {
            q.push(pw[i - 1]);
        } else {
            let a = (u - knots[i]) / (knots[i + p] - knots[i]);
            let mut r = [0.0; 3];
            for c in 0..3 {
                r[c] = a * pw[i][c] + (1.0 - a) * pw[i - 1][c];
            }
            q.push(r);
        }
    }
    let mut kn = knots.to_vec();
    kn.insert(k + 1, u);
    (kn, q)
}

/// Rational quadratic arc of radius `r` (origin-centred) from angle `a` to
/// `b` (`|b − a| < π`): control points and weights.
fn arc3(r: f64, a: f64, b: f64) -> ([[f64; 2]; 3], [f64; 3]) {
    let (h, m) = (0.5 * (b - a), 0.5 * (a + b));
    let rm = r / h.cos();
    (
        [
            [r * a.cos(), r * a.sin()],
            [rm * m.cos(), rm * m.sin()],
            [r * b.cos(), r * b.sin()],
        ],
        [1.0, h.cos(), 1.0],
    )
}

/// One spline piece of the rotor surface: indices into the 18 points.
struct SurfacePiece {
    name: &'static str,
    first: usize,
    knots: Vec<f64>,
    weights: Vec<f64>,
}

/// The rotor surface at every design (lab frame): the 18 control points in
/// ascending angle and the five pieces.
fn rotor_surface() -> (Vec<[f64; 2]>, Vec<SurfacePiece>) {
    let (a, t2, t1) = (HALF_POLE, POCKET_ANGLE_INNER, POCKET_ANGLE_OUTER);
    let quad = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(N_ROTOR_POINTS);
    let mut pieces = Vec::new();
    let mut push = |cps: &[[f64; 2]], w: Vec<f64>, knots: Vec<f64>, name: &'static str| {
        let first = if pts.is_empty() { 0 } else { pts.len() - 1 };
        for (j, q) in cps.iter().enumerate() {
            if j == 0 && !pts.is_empty() {
                continue; // shared with the previous piece
            }
            pts.push(rot(*q, ROTOR_FRAME));
        }
        pieces.push(SurfacePiece {
            name,
            first,
            knots,
            weights: w,
        });
    };
    let (c, w) = arc3(R_ROTOR, -a, -t2);
    push(&c, w.to_vec(), quad.clone(), "rotor.surface_edge_lo");
    let (c, w) = arc3(R_ROTOR, -t2, -t1);
    push(&c, w.to_vec(), quad.clone(), "rotor.surface_bridge_lo");
    // Pole arc: one rational quadratic segment, refined by uniform knot
    // insertion (each interior knot once).
    let (c, w) = arc3(R_ROTOR, -t1, t1);
    let mut kn = quad.clone();
    let mut pw: Vec<[f64; 3]> = c
        .iter()
        .zip(w)
        .map(|(q, wi)| [q[0] * wi, q[1] * wi, wi])
        .collect();
    let m = POLE_ARC_INTERIOR_KNOTS + 1;
    for j in 1..m {
        let (k2, p2) = insert_knot(2, &kn, &pw, j as f64 / m as f64);
        kn = k2;
        pw = p2;
    }
    let cps: Vec<[f64; 2]> = pw.iter().map(|q| [q[0] / q[2], q[1] / q[2]]).collect();
    let ws: Vec<f64> = pw.iter().map(|q| q[2]).collect();
    push(&cps, ws, kn, "rotor.surface_pole");
    let (c, w) = arc3(R_ROTOR, t1, t2);
    push(&c, w.to_vec(), quad.clone(), "rotor.surface_bridge_hi");
    let (c, w) = arc3(R_ROTOR, t2, a);
    push(&c, w.to_vec(), quad, "rotor.surface_edge_hi");
    debug_assert_eq!(pts.len(), N_ROTOR_POINTS);
    (pts, pieces)
}

/// Offset group of rotor surface point `i` (ascending angle).
fn rotor_group(i: usize) -> usize {
    let j = i.min(N_ROTOR_POINTS - 1 - i); // mirror pairs
    match j {
        0 => 0,
        1 => 1,
        2..=4 => 2,
        5 => 3,
        6 => 4,
        7 => 5,
        _ => 6,
    }
}

/// Bore control points of one slot at opening width `sw2` in the slot frame:
/// `[tooth_lo, mid_lo, corner_lo, open_mid, corner_hi, mid_hi, tooth_hi]`,
/// with the weights of the lo arc's middle and the opening's middle.
fn bore_slot<T: Real>(sw2: T) -> ([[T; 2]; 7], T, T) {
    let c = T::cst;
    let half = c(0.5);
    let alpha = c(HALF_SLOT);
    let s = sw2 * half;
    let phi_o = (s / c(R_BORE)).asin();
    let h = (alpha - phi_o) * half;
    let m = (alpha + phi_o) * half;
    let rm = c(R_BORE) / h.cos();
    let o_hi = [c(R_BORE) * phi_o.cos(), s];
    let pts = [
        polar(c(R_BORE), -alpha),
        polar(rm, -m),
        mirror(o_hi),
        [c(R_BORE) / phi_o.cos(), c(0.0)],
        o_hi,
        polar(rm, m),
        polar(c(R_BORE), alpha),
    ];
    (pts, h.cos(), phi_o.cos())
}

/// Lab-frame bore control points at `sw2` (offsets not applied), in the
/// order of [`control_points`]: tooth 0, then per slot `mid_lo, corner_lo,
/// open_mid, corner_hi, mid_hi`, then the next tooth. Returns also the
/// per-slot middle weights.
fn bore_points<T: Real>(sw2: T) -> (Vec<[T; 2]>, T, T) {
    let (cell, w_mid, w_open) = bore_slot(sw2);
    let mut out = Vec::with_capacity(N_STATOR_POINTS);
    for k in 0..SLOTS_IN_SECTOR {
        let angle = STATOR_FRAME0 + 2.0 * HALF_SLOT * k as f64;
        if k == 0 {
            out.push(rot(cell[0], angle));
        }
        for q in &cell[1..6] {
            out.push(rot(*q, angle));
        }
        if k + 1 == SLOTS_IN_SECTOR {
            out.push(rot(cell[6], angle));
        } else {
            // The next slot's first tooth point (one value for both arcs).
            let next = STATOR_FRAME0 + 2.0 * HALF_SLOT * (k + 1) as f64;
            out.push(rot(cell[0], next));
        }
    }
    (out, w_mid, w_open)
}

/// Offset group of stator bore point `i` ([`control_points`] order).
fn stator_group(i: usize) -> usize {
    match i % 6 {
        0 => N_ROTOR_GROUPS + 2,     // tooth centre: CPS3
        1 | 5 => N_ROTOR_GROUPS + 1, // tooth-arc middles: CPS2
        _ => N_ROTOR_GROUPS,         // corners and opening middle: CPS1
    }
}

fn unit(p: [f64; 2]) -> [f64; 2] {
    let r = p[0].hypot(p[1]);
    [p[0] / r, p[1] / r]
}

/// The 55 free-form control points: the 18 rotor surface points (ascending
/// angle), then the 37 stator bore points (tooth 0, then per slot the arc
/// middle, corner, opening middle, corner, arc middle, and the next tooth).
pub fn control_points() -> Vec<ControlPoint> {
    let (rp, _) = rotor_surface();
    let mut out: Vec<ControlPoint> = rp
        .iter()
        .enumerate()
        .map(|(i, &p)| ControlPoint {
            part: CpPart::Rotor,
            label: format!("rotor.p{i:02}"),
            group: rotor_group(i),
            direction: unit(p),
        })
        .collect();
    let (sp, _, _) = bore_points(SW2_GENERATOR_DEFAULT);
    let names = ["mid_lo", "corner_lo", "open_mid", "corner_hi", "mid_hi"];
    for (i, &p) in sp.iter().enumerate() {
        let label = if i % 6 == 0 {
            format!("tooth{}", i / 6)
        } else {
            format!("slot{}.{}", i / 6, names[i % 6 - 1])
        };
        out.push(ControlPoint {
            part: CpPart::Stator,
            label,
            group: stator_group(i),
            direction: unit(p),
        });
    }
    out
}

/// Positions of the 55 control points at `p` ([`control_points`] order):
/// the generator's points at the scalars plus `δ_g u_i`.
pub fn control_point_positions<T: Real>(p: &[T; N_CP_PARAMS]) -> Vec<[T; 2]> {
    let cps = control_points();
    let (rp, _) = rotor_surface();
    let (sp, _, _) = bore_points(p[IcemDesign::index_of("SW2").expect("SW2")]);
    rp.iter()
        .map(|q| [T::cst(q[0]), T::cst(q[1])])
        .chain(sp)
        .zip(&cps)
        .map(|(q, cp)| {
            let d = p[N_PARAMS + cp.group];
            [
                q[0] + d * T::cst(cp.direction[0]),
                q[1] + d * T::cst(cp.direction[1]),
            ]
        })
        .collect()
}

fn spline<T: Real>(name: &str, knots: Vec<f64>, cps: Vec<[T; 2]>, weights: Vec<T>) -> Curve<T> {
    Curve {
        name: name.to_string(),
        shape: CurveShape::Spline {
            degree: 2,
            knots,
            cps,
            weights,
        },
    }
}

/// Every material interface and domain boundary of the sector at the 19
/// components `p` (lab frame, rotor angle 0): [`sector_curves`] at the 9
/// scalars with the free-form rotor surface, bore and the curves that meet
/// them replaced (module documentation).
///
/// # Errors
///
/// As [`sector_curves`]; also [`IcemParamError::Construction`] if a rotor
/// surface point reaches the coupling circle or a bore point falls inside
/// it.
pub fn sector_curves_cp<T: Real>(p: &[T; N_CP_PARAMS]) -> Result<Vec<Curve<T>>, IcemParamError> {
    let scalars: [T; N_PARAMS] = std::array::from_fn(|k| p[k]);
    let base = sector_curves(&scalars)?;
    let pts = control_point_positions(p);
    let (rp, rs) = pts.split_at(N_ROTOR_POINTS);
    for (q, part) in pts.iter().zip(control_points()) {
        let r = q[0].value().hypot(q[1].value());
        let bad = match part.part {
            CpPart::Rotor => r >= R_COUPLING,
            CpPart::Stator => r <= R_COUPLING,
        };
        if bad {
            return Err(IcemParamError::Construction(format!(
                "control point {} at r = {r:.6e} m crosses the coupling circle",
                part.label
            )));
        }
    }
    let (_, pieces) = rotor_surface();
    let sw2 = p[IcemDesign::index_of("SW2").expect("SW2")];
    let (_, w_mid, w_open) = bore_points(sw2);
    let one = T::cst(1.0);
    let quad = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let mut out = Vec::with_capacity(base.len() + 8);
    for cv in base {
        let name = cv.name.as_str();
        if name == "rotor.surface" {
            for pc in &pieces {
                let n = pc.weights.len();
                out.push(spline(
                    pc.name,
                    pc.knots.clone(),
                    rp[pc.first..pc.first + n].to_vec(),
                    pc.weights.iter().map(|&w| T::cst(w)).collect(),
                ));
            }
            continue;
        }
        if let Some(side) = name.strip_prefix("rotor.edge_") {
            let q = if side == "lo" {
                rp[0]
            } else {
                rp[N_ROTOR_POINTS - 1]
            };
            out.push(line(format!("{name}_iron"), cv.start(), q));
            out.push(line(format!("{name}_gap"), q, cv.end()));
            continue;
        }
        if let Some((slot, part)) = name
            .strip_prefix("slot")
            .and_then(|rest| rest.split_once('.'))
        {
            let k: usize = slot.parse().expect("slot index");
            let b = 6 * k;
            match part {
                "bore_lo" => out.push(spline(
                    name,
                    quad.clone(),
                    rs[b..b + 3].to_vec(),
                    vec![one, w_mid, one],
                )),
                "bore_open" => out.push(spline(
                    name,
                    quad.clone(),
                    rs[b + 2..b + 5].to_vec(),
                    vec![one, w_open, one],
                )),
                "bore_hi" => out.push(spline(
                    name,
                    quad.clone(),
                    rs[b + 4..b + 7].to_vec(),
                    vec![one, w_mid, one],
                )),
                "channel_lo" => out.push(line(name.to_string(), rs[b + 2], cv.end())),
                "channel_hi" => out.push(line(name.to_string(), rs[b + 4], cv.end())),
                _ => out.push(cv),
            }
            continue;
        }
        let tooth_lo = rs[0];
        let tooth_hi = rs[N_STATOR_POINTS - 1];
        match name {
            "stator.edge_lo_iron" => out.push(line(name.to_string(), tooth_lo, cv.end())),
            "stator.edge_hi_iron" => out.push(line(name.to_string(), tooth_hi, cv.end())),
            "stator.edge_lo_gap" => out.push(line(name.to_string(), cv.start(), tooth_lo)),
            "stator.edge_hi_gap" => out.push(line(name.to_string(), cv.start(), tooth_hi)),
            _ => out.push(cv),
        }
    }
    Ok(out)
}

/// [`sector_curves_cp`] in `f64`.
///
/// # Errors
///
/// As [`sector_curves_cp`].
pub fn sector_curves_cp_f64(design: &IcemCpDesign) -> Result<Vec<Curve<f64>>, IcemParamError> {
    sector_curves_cp(&design.values)
}

/// [`sector_curves_cp`] with the derivative of every coordinate with respect
/// to the 19 components.
///
/// # Errors
///
/// As [`sector_curves_cp`].
pub fn sector_curves_cp_dual(design: &IcemCpDesign) -> Result<Vec<Curve<CpDual>>, IcemParamError> {
    let p: [CpDual; N_CP_PARAMS] = std::array::from_fn(|k| CpDual::var(design.values[k], k));
    sector_curves_cp(&p)
}

// ── Offset fit ──────────────────────────────────────────────────────────

/// Least-squares offsets of observed control-point positions against the
/// construction at fixed scalars.
#[derive(Clone, Debug, PartialEq)]
pub struct OffsetFit {
    /// Fitted `δ_g` (m), [`OFFSET_NAMES`] order.
    pub offsets: [f64; N_OFFSETS],
    /// Per group, the largest `|P_obs − (P_g(θ) + δ_g u)|` (m) over its
    /// points: the fit residual.
    pub residual: [f64; N_OFFSETS],
    /// Per group, the largest component of the observed displacement
    /// normal to the offset direction (m).
    pub normal: [f64; N_OFFSETS],
    /// Per group, whether `δ_g` lies in [`OFFSET_BOUNDS`].
    pub within_bounds: [bool; N_OFFSETS],
}

/// Fit the 10 offsets to `observed` control-point positions
/// ([`control_points`] order) with the scalars fixed at `scalars`. With
/// unit directions the least-squares offset of a group is the mean of the
/// projections `u_i · (P_obs,i − P_i(θ))`.
///
/// # Panics
///
/// If `observed` does not have one position per control point.
pub fn fit_offsets(scalars: &IcemDesign, observed: &[[f64; 2]]) -> OffsetFit {
    let cps = control_points();
    assert_eq!(
        observed.len(),
        cps.len(),
        "one observation per control point"
    );
    let zero = IcemCpDesign::from_parts(scalars, &[0.0; N_OFFSETS]);
    let model = control_point_positions(&zero.values);
    let mut sum = [0.0; N_OFFSETS];
    let mut count = [0usize; N_OFFSETS];
    for ((cp, q), m) in cps.iter().zip(observed).zip(&model) {
        let d = [q[0] - m[0], q[1] - m[1]];
        sum[cp.group] += d[0] * cp.direction[0] + d[1] * cp.direction[1];
        count[cp.group] += 1;
    }
    let offsets: [f64; N_OFFSETS] = std::array::from_fn(|g| sum[g] / count[g] as f64);
    let mut residual = [0.0f64; N_OFFSETS];
    let mut normal = [0.0f64; N_OFFSETS];
    for ((cp, q), m) in cps.iter().zip(observed).zip(&model) {
        let d = [q[0] - m[0], q[1] - m[1]];
        let g = cp.group;
        let e = [
            d[0] - offsets[g] * cp.direction[0],
            d[1] - offsets[g] * cp.direction[1],
        ];
        residual[g] = residual[g].max(e[0].hypot(e[1]));
        normal[g] = normal[g].max((d[1] * cp.direction[0] - d[0] * cp.direction[1]).abs());
    }
    let within_bounds =
        std::array::from_fn(|g| (OFFSET_BOUNDS[g].0..=OFFSET_BOUNDS[g].1).contains(&offsets[g]));
    OffsetFit {
        offsets,
        residual,
        normal,
        within_bounds,
    }
}

// ── Node motion ─────────────────────────────────────────────────────────

/// The radial sector-edge pair that slides under [`EdgeRule::Slide`]: the
/// iron part of the stator edges, as in the 9-parameter map. The rotor
/// edges keep the prescribed stretch: their iron part ends at the moving
/// surface point (`CPR1`), and a sliding rotor edge folds the iron next to
/// that corner (measured in `benchmarks/motor_reference/parametric_cp/`).
const CP_SLIDE_EDGES: [(&str, &str); 1] = [("stator.edge_lo_iron", "stator.edge_hi_iron")];

/// Default extension options of [`IcemCpShapeMap`]: sliding stator edges and
/// the plain harmonic extension (`χ = 0`). With the Jacobian stiffening of the
/// 9-parameter map (`χ = 1`) the full iteration-120 design (12 scalars and
/// the fitted offsets) inverts triangles at the flux-barrier pockets' outer
/// corners from L1 on; with `χ = 0` it moves L0 to L2 without inversion
/// (`benchmarks/motor_reference/parametric_cp/`, all variants measured).
pub const CP_MOTION_DEFAULT: MotionOptions = MotionOptions {
    edges: EdgeRule::Slide,
    stiffening: 0.0,
};

/// The fixed-topology node motion `θ ↦ X(θ)` over the 19 geometric
/// components (module documentation).
pub struct IcemCpShapeMap {
    theta0: IcemCpDesign,
    core: MotionCore,
}

impl IcemCpShapeMap {
    /// Attach `tagged` (meshed at `theta0`) to [`sector_curves_cp`] and
    /// factor the extension operator, with [`CP_MOTION_DEFAULT`].
    ///
    /// # Errors
    ///
    /// As [`super::icem_parametric::IcemShapeMap::new`].
    pub fn new(tagged: &TaggedTriMesh, theta0: IcemCpDesign) -> Result<Self, IcemParamError> {
        Self::with_options(tagged, theta0, CP_MOTION_DEFAULT)
    }

    /// [`IcemCpShapeMap::new`] with explicit [`MotionOptions`]
    /// ([`EdgeRule::Stretch`] prescribes a stretch on both radial edges).
    ///
    /// # Errors
    ///
    /// As [`IcemCpShapeMap::new`].
    pub fn with_options(
        tagged: &TaggedTriMesh,
        theta0: IcemCpDesign,
        options: MotionOptions,
    ) -> Result<Self, IcemParamError> {
        let curves = sector_curves_cp_f64(&theta0)?;
        let core = MotionCore::build(tagged, &curves, options, &CP_SLIDE_EDGES)?;
        Ok(Self { theta0, core })
    }

    /// The design the mesh was generated at.
    pub fn theta0(&self) -> IcemCpDesign {
        self.theta0
    }

    /// The extension options.
    pub fn options(&self) -> MotionOptions {
        self.core.options()
    }

    /// Number of sliding antiperiodic node pairs (stator and rotor edges).
    pub fn slide_pairs(&self) -> usize {
        self.core.slide_pairs()
    }

    /// The `θ0` mesh (counter-clockwise triangles).
    pub fn base(&self) -> &TriMesh {
        self.core.base()
    }

    /// Per-triangle physical tags.
    pub fn tri_tags(&self) -> &[i32] {
        self.core.tri_tags()
    }

    /// Curve names, parallel to [`sector_curves_cp`].
    pub fn curve_names(&self) -> &[String] {
        self.core.curve_names()
    }

    /// Per-node attachment.
    pub fn attachments(&self) -> &[NodeAttachment] {
        self.core.attachments()
    }

    /// Attachment statistics.
    pub fn report(&self) -> &AttachmentReport {
        self.core.report()
    }

    /// Node coordinates `X(θ)`.
    ///
    /// # Errors
    ///
    /// [`IcemParamError::Construction`] if `theta` defines no geometry.
    pub fn positions(&self, theta: &IcemCpDesign) -> Result<Vec<[f64; 2]>, IcemParamError> {
        Ok(self.core.positions_from(&sector_curves_cp_f64(theta)?))
    }

    /// `∂X/∂θ_k` at `θ` for the 19 components ([`cp_param_names`] order):
    /// `out[k][node] = [∂x/∂θ_k, ∂y/∂θ_k]`.
    ///
    /// # Errors
    ///
    /// [`IcemParamError::Construction`] if `theta` defines no geometry.
    pub fn jacobian(&self, theta: &IcemCpDesign) -> Result<Vec<Vec<[f64; 2]>>, IcemParamError> {
        Ok(self.core.jacobian_from(&sector_curves_cp_dual(theta)?))
    }

    /// The `θ0` mesh with its nodes at `X(θ)`.
    ///
    /// # Errors
    ///
    /// As [`IcemCpShapeMap::positions`].
    pub fn moved_mesh(&self, theta: &IcemCpDesign) -> Result<TriMesh, IcemParamError> {
        Ok(TriMesh {
            nodes: self.positions(theta)?,
            tris: self.base().tris.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn counts_and_groups_match_the_upstream_registration() {
        let cps = control_points();
        assert_eq!(cps.len(), N_ROTOR_POINTS + N_STATOR_POINTS);
        for g in 0..N_ROTOR_GROUPS {
            let n = cps.iter().filter(|c| c.group == g).count();
            assert_eq!(n, UPSTREAM_ROTOR_POINTS[g].len(), "{}", OFFSET_NAMES[g]);
        }
        for g in 0..N_STATOR_GROUPS {
            let n = cps.iter().filter(|c| c.group == N_ROTOR_GROUPS + g).count();
            assert_eq!(
                n,
                UPSTREAM_STATOR_POINTS[g].len(),
                "{}",
                OFFSET_NAMES[N_ROTOR_GROUPS + g]
            );
        }
    }

    #[test]
    fn rotor_surface_is_the_circle_at_zero_offset() {
        let curves = sector_curves_cp_f64(&IcemCpDesign::INITIAL).unwrap();
        let mut worst: f64 = 0.0;
        let mut pieces = 0;
        for c in curves
            .iter()
            .filter(|c| c.name.starts_with("rotor.surface_"))
        {
            pieces += 1;
            for j in 0..=200 {
                let q = c.point(j as f64 / 200.0);
                worst = worst.max((q[0].hypot(q[1]) - R_ROTOR).abs());
            }
        }
        assert_eq!(pieces, 5);
        assert!(worst < 1e-16, "{worst:e}");
        // The pieces are contiguous from 60 to 120 degrees.
        let first = curves
            .iter()
            .find(|c| c.name == "rotor.surface_edge_lo")
            .unwrap();
        let last = curves
            .iter()
            .find(|c| c.name == "rotor.surface_edge_hi")
            .unwrap();
        let ang = |q: [f64; 2]| q[1].atan2(q[0]);
        assert!((ang(first.start()) - PI / 3.0).abs() < 1e-15);
        assert!((ang(last.end()) - 2.0 * PI / 3.0).abs() < 1e-15);
    }

    #[test]
    fn edges_are_rotation_images_under_offsets() {
        let mut d = IcemCpDesign::INITIAL;
        d.values[N_PARAMS] = -1.2e-3; // CPR1
        d.values[N_PARAMS + 9] = 3e-4; // CPS3
        let pts = control_point_positions(&d.values);
        let (c, s) = ((PI / 3.0).cos(), (PI / 3.0).sin());
        for (lo, hi) in [(0, N_ROTOR_POINTS - 1), (N_ROTOR_POINTS, pts.len() - 1)] {
            let a = pts[lo];
            let b = pts[hi];
            let e = (c * a[0] - s * a[1] - b[0]).hypot(s * a[0] + c * a[1] - b[1]);
            assert!(e < 1e-16, "{e:e}");
        }
    }

    #[test]
    fn fit_recovers_synthetic_offsets() {
        let mut d = IcemCpDesign::INITIAL;
        for (g, &(lo, hi)) in OFFSET_BOUNDS.iter().enumerate() {
            d.values[N_PARAMS + g] = 0.5 * (lo + hi);
        }
        d.values[4] = 0.0015; // SW2: directions stay those of 2.3 mm
        let obs = control_point_positions(&d.values);
        let fit = fit_offsets(&d.scalars(), &obs);
        for g in 0..N_OFFSETS {
            assert!((fit.offsets[g] - d.values[N_PARAMS + g]).abs() < 1e-16);
            assert!(fit.residual[g] < 1e-16);
            assert!(fit.within_bounds[g]);
        }
    }
}
