//! **Junction-pinned multi-parameter morph** of the grounded-transmon fixture
//! (Epic #569 / umbrella #1034, issue #1035, Phase A).
//!
//! #589 and #594 optimized one parameter, an island scale about the centroid.
//! That map moves the island's junction-attachment nodes ~225 μm per unit θ
//! while the grounded side of the junction sits ~1 μm away and stays fixed, so
//! the ~0.7 μm junction-region tets set the deformation budget. The three
//! parameters here each have a harmonic morph field `D_i` that is **exactly
//! zero on the junction neighborhood**, removing that failure mode:
//!
//! * **`theta_L`, island length.** Island nodes with `y ≥ y_pin` move by
//!   `D = (0, y − y_pin, 0)`; island nodes below `y_pin` (the junction lead
//!   and the base of the pad) stay fixed. `θ = −0.1` shortens the pad above
//!   `y_pin` by 10%.
//! * **`theta_W`, island width.** `D = ((x − x_axis)·s(y), 0, 0)` on the same
//!   nodes, with `s` ramping linearly from 0 at `y_pin` to 1 at
//!   `y_pin + width_ramp`, so the junction lead is not widened.
//! * **`theta_G`, cutout gap.** The ground nodes on the straight cutout edges
//!   (`|x| = x_edge`) move outward, `D = (sign(x)·gap_scale·τ(y), 0, 0)`, with
//!   a trapezoidal taper `τ` that is 0 at both ends of the run. A band of
//!   ground-sheet nodes behind the edge is released from the morph's fixed
//!   set, so the motion spreads in-plane through the sheet; moving a metal
//!   node inside its own sheet does not change the conductor, except at the
//!   edge being moved. `θ = +0.1` widens the gap by 10%.
//!
//! Every field is built by [`super::harmonic_dirichlet_velocity`] and has
//! zero prescribed `z` velocity, so `D_z ≡ 0`: the metal sheets and the
//! `z = 0` substrate interface stay planar.
//!
//! # Built for re-morphing (Phase B, #1036)
//!
//! The node **sets** ([`TransmonMorphRoles`]) are identified once, from the
//! base geometry. The Dirichlet **data** is evaluated by
//! [`parameter_fields`] from the coordinates of whatever mesh is passed in,
//! so a re-morphing optimizer can rebuild `D_i(X_k)` at every iterate. The
//! only constants are `y_pin` (a pinned material point: no field moves the
//! nodes below it) and `gap_scale` (the normalization unit of `theta_G`).
//!
//! # Junction leads are excluded
//!
//! `E_J` comes from the lumped `L_J = 14.860 nH` and does not depend on
//! geometry, the 1 μm leads contribute negligible capacitance, and their
//! ~0.7 μm tets are exactly what limited #589 / #594. No parameter moves them.

use std::collections::BTreeSet;

use super::{harmonic_dirichlet_velocity, min_tet_volume_ratio, worst_tet_volume_ratio};
use crate::assembly::electrostatic::ElectrostaticError;
use crate::mesh::{MetalRole, TetMesh, TransmonFixture};

/// The three parameter names, in [`parameter_fields`] column order.
pub const PARAM_NAMES: [&str; 3] = ["theta_L", "theta_W", "theta_G"];

/// Geometric knobs of the parameterization and of the partial-charge regions,
/// in the units of the mesh the roles are identified on.
#[derive(Clone, Copy, Debug)]
pub struct TransmonMorphSpec {
    /// Island nodes with `y < y_pin` never move (junction lead + pad base).
    pub y_pin: f64,
    /// `theta_W` ramps its x-scale from 0 at `y_pin` to 1 at `y_pin + width_ramp`.
    pub width_ramp: f64,
    /// The straight cutout-edge run `[y0, y1]` that `theta_G` moves. It must
    /// end where the ground behind the edge is still a full sheet (on the
    /// fixture the sheet narrows to a 2 μm strip above `y ≈ 535`).
    pub gap_run: (f64, f64),
    /// Length of the linear taper of `theta_G` at each end of the run.
    pub gap_taper: f64,
    /// Width of the released ground-sheet band behind each cutout edge.
    pub band_width: f64,
    /// Half-width of the partial-charge window around the cutout (`|x| ≤`).
    pub charge_window_x: f64,
    /// `y` extent of the partial-charge window.
    pub charge_window_y: (f64, f64),
    /// The junction-end charge region is `y < y_pin + junction_end_dy`.
    pub junction_end_dy: f64,
    /// Ground in the window above `claw_y` is the claw region.
    pub claw_y: f64,
}

impl TransmonMorphSpec {
    /// The values used for the committed `transmon_smoke` fixture, for a mesh
    /// whose coordinates are the fixture's micrometres times `m_per_unit`
    /// (`1e-6` for a mesh rescaled to metres, `1.0` for the raw fixture).
    ///
    /// They come from probing the fixture (issue #1035): island x ∈ [−12, 12],
    /// y ∈ [6.5, 632] μm, pad base at y = 12 with node levels up to 19.7 μm
    /// and then a jump to 24.6 μm; cutout rectangle x ∈ [−42, 42],
    /// y ∈ [0, 662] μm, backed by full ground sheet only below y ≈ 535 μm
    /// (above it a 2 μm ground strip separates the cutout from the claw);
    /// claw x ∈ [50, 84] μm wrapping the island top at y ∈ [541, 704] μm.
    pub fn fixture_default(m_per_unit: f64) -> Self {
        let u = m_per_unit;
        Self {
            y_pin: 20.0 * u,
            width_ramp: 20.0 * u,
            gap_run: (0.0, 520.0 * u),
            gap_taper: 60.0 * u,
            band_width: 40.0 * u,
            charge_window_x: 100.0 * u,
            charge_window_y: (-60.0 * u, 760.0 * u),
            junction_end_dy: 20.0 * u,
            claw_y: 520.0 * u,
        }
    }
}

/// Node sets of the parameterization, identified topologically once from the
/// base geometry (see the module docs).
#[derive(Clone, Debug)]
pub struct TransmonMorphRoles {
    /// The spec the roles were identified with.
    pub spec: TransmonMorphSpec,
    /// Island conductor nodes.
    pub island: Vec<u32>,
    /// Island nodes with `y < y_pin` (never move).
    pub island_lead: Vec<u32>,
    /// Island nodes with `y ≥ y_pin` (moved by `theta_L` and `theta_W`).
    pub island_body: Vec<u32>,
    /// Ground conductor nodes (ground plane + shorted resonator + claw).
    pub ground: Vec<u32>,
    /// Feedline conductor nodes.
    pub feedline: Vec<u32>,
    /// Nodes of the `lumped_element` (junction) sheet.
    pub junction: Vec<u32>,
    /// Nodes on the far bounding-box faces.
    pub far: Vec<u32>,
    /// Ground nodes on the straight cutout edges inside `gap_run` (the
    /// moving Dirichlet set of `theta_G`).
    pub gap_edge: Vec<u32>,
    /// Ground-sheet nodes released (free) in the `theta_G` morph.
    pub gap_band: Vec<u32>,
    /// `|x|` of the cutout edges.
    pub x_edge: f64,
    /// Island half-width (max `|x − x_axis|` over the island).
    pub island_half_width: f64,
    /// The `theta_G` normalization unit: the base cutout gap
    /// `x_edge − island_half_width`.
    pub gap_scale: f64,
}

fn bbox(mesh: &TetMesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &mesh.nodes {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    (lo, hi)
}

fn err(msg: String) -> ElectrostaticError {
    ElectrostaticError::ShapeMismatch(msg)
}

impl TransmonMorphRoles {
    /// Identify the node sets on `mesh` (the fixture mesh, possibly rescaled:
    /// it must share `fx.mesh`'s node numbering).
    ///
    /// Checks, returning an error on failure: the conductors split into
    /// ground / island / feedline; the cutout edge exists on both sides; every
    /// edge node lies on the ground sheet's outline; no band node does (so
    /// the band moves only inside the sheet); and the edge, band, junction
    /// and far sets are disjoint where they must be.
    ///
    /// # Errors
    ///
    /// [`ElectrostaticError::ShapeMismatch`] if any check fails.
    pub fn identify(
        fx: &TransmonFixture,
        mesh: &TetMesh,
        spec: TransmonMorphSpec,
    ) -> Result<Self, ElectrostaticError> {
        if mesh.n_nodes() != fx.mesh.n_nodes() {
            return Err(err(format!(
                "identify: mesh has {} nodes, fixture {}",
                mesh.n_nodes(),
                fx.mesh.n_nodes()
            )));
        }
        let comps = fx.split_metal_conductors();
        let pick = |role: MetalRole| {
            comps
                .iter()
                .find(|c| c.role == role)
                .ok_or_else(|| err(format!("identify: no {role:?} conductor")))
        };
        let ground_c = pick(MetalRole::Ground)?;
        let island = pick(MetalRole::Island)?.nodes.clone();
        let feedline = pick(MetalRole::Feedline)?.nodes.clone();
        let ground = ground_c.nodes.clone();
        let p = |n: u32| mesh.nodes[n as usize];

        let (lo, hi) = bbox(mesh);
        let extent = (0..3).map(|d| hi[d] - lo[d]).fold(0.0_f64, f64::max);
        let tol = 1e-9 * extent.max(f64::MIN_POSITIVE);

        let mut junction: Vec<u32> = fx
            .lumped_element_triangles()
            .into_iter()
            .flatten()
            .collect();
        junction.sort_unstable();
        junction.dedup();

        let far: Vec<u32> = (0..mesh.n_nodes() as u32)
            .filter(|&n| {
                let q = p(n);
                (0..3).any(|d| (q[d] - lo[d]).abs() < tol || (q[d] - hi[d]).abs() < tol)
            })
            .collect();

        let (island_lead, island_body): (Vec<u32>, Vec<u32>) =
            island.iter().partition(|&&n| p(n)[1] < spec.y_pin);
        if island_body.is_empty() || island_lead.is_empty() {
            return Err(err(format!(
                "identify: y_pin {} does not split the island ({} lead / {} body nodes)",
                spec.y_pin,
                island_lead.len(),
                island_body.len()
            )));
        }
        let x_axis = island_axis(mesh, &island);
        let island_half_width = island
            .iter()
            .map(|&n| (p(n)[0] - x_axis).abs())
            .fold(0.0_f64, f64::max);

        // The cutout edge: the ground sheet node nearest the island axis,
        // strictly inside the run (the run ends may lie on the cutout's
        // bottom / top edges, whose nodes sit closer to the axis), beyond the
        // island.
        let (y0, y1) = spec.gap_run;
        let sheet = |n: u32| p(n)[2].abs() <= tol;
        let in_run = |n: u32| p(n)[1] >= y0 - tol && p(n)[1] <= y1 + tol;
        let x_edge = ground
            .iter()
            .copied()
            .filter(|&n| sheet(n) && p(n)[1] > y0 + tol && p(n)[1] < y1 - tol)
            .map(|n| (p(n)[0] - x_axis).abs())
            .filter(|&ax| ax > island_half_width + tol)
            .fold(f64::INFINITY, f64::min);
        if !x_edge.is_finite() {
            return Err(err("identify: no ground cutout edge in gap_run".to_string()));
        }
        let gap_edge: Vec<u32> = ground
            .iter()
            .copied()
            .filter(|&n| sheet(n) && in_run(n) && ((p(n)[0] - x_axis).abs() - x_edge).abs() <= tol)
            .collect();
        let n_pos = gap_edge.iter().filter(|&&n| p(n)[0] > x_axis).count();
        if n_pos == 0 || n_pos == gap_edge.len() {
            return Err(err(
                "identify: the cutout edge must be found on both sides of the island".to_string(),
            ));
        }
        let gap_band: Vec<u32> = ground
            .iter()
            .copied()
            .filter(|&n| {
                let ax = (p(n)[0] - x_axis).abs();
                sheet(n)
                    && ax > x_edge + tol
                    && ax <= x_edge + spec.band_width
                    && p(n)[1] > y0 + tol
                    && p(n)[1] < y1 - tol
            })
            .collect();

        // Outline of the ground sheet: nodes on triangle edges used once.
        let mut edge_count: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for t in &ground_c.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edge_count.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let outline: BTreeSet<u32> = edge_count
            .iter()
            .filter(|(_, c)| **c == 1)
            .flat_map(|((a, b), _)| [*a, *b])
            .collect();
        if let Some(&n) = gap_edge.iter().find(|n| !outline.contains(n)) {
            return Err(err(format!(
                "identify: cutout-edge node {n} at {:?} is not on the ground outline",
                p(n)
            )));
        }
        if let Some(&n) = gap_band.iter().find(|n| outline.contains(n)) {
            return Err(err(format!(
                "identify: band node {n} at {:?} lies on the ground outline (it would \
                 reshape the conductor); shrink band_width or gap_run",
                p(n)
            )));
        }
        let guard: BTreeSet<u32> = junction.iter().chain(&far).copied().collect();
        if gap_edge.iter().chain(&gap_band).any(|n| guard.contains(n)) {
            return Err(err(
                "identify: the theta_G edge/band touches the junction or the far faces".to_string(),
            ));
        }

        Ok(Self {
            spec,
            island,
            island_lead,
            island_body,
            ground,
            feedline,
            junction,
            far,
            gap_edge,
            gap_band,
            x_edge,
            island_half_width,
            gap_scale: x_edge - island_half_width,
        })
    }

    /// The nodes every field holds at exactly zero: ground (minus, for
    /// `theta_G`, its edge and band), feedline, junction, far faces, and the
    /// island part that the field does not move.
    fn fixed_zero(&self, param: usize) -> Vec<u32> {
        let mut set: BTreeSet<u32> = BTreeSet::new();
        set.extend(&self.ground);
        set.extend(&self.feedline);
        set.extend(&self.junction);
        set.extend(&self.far);
        if param == 2 {
            set.extend(&self.island);
            for n in self.gap_edge.iter().chain(&self.gap_band) {
                set.remove(n);
            }
        } else {
            set.extend(&self.island_lead);
        }
        set.into_iter().collect()
    }

    /// The nodes the field of `param` must hold at exactly zero (the
    /// audited set of [`audit_field`]); identical to the morph's fixed set.
    pub fn zero_set(&self, param: usize) -> Vec<u32> {
        self.fixed_zero(param)
    }

    /// The moving Dirichlet set of `param` (island body for `theta_L` /
    /// `theta_W`, cutout edge for `theta_G`).
    pub fn moving_set(&self, param: usize) -> &[u32] {
        if param == 2 {
            &self.gap_edge
        } else {
            &self.island_body
        }
    }
}

fn island_axis(mesh: &TetMesh, island: &[u32]) -> f64 {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &n in island {
        let x = mesh.nodes[n as usize][0];
        lo = lo.min(x);
        hi = hi.max(x);
    }
    0.5 * (lo + hi)
}

/// Trapezoidal taper of `theta_G`: 0 at both run ends, 1 in the middle.
fn gap_taper(y: f64, spec: &TransmonMorphSpec) -> f64 {
    let (y0, y1) = spec.gap_run;
    ((y - y0) / spec.gap_taper)
        .min((y1 - y) / spec.gap_taper)
        .clamp(0.0, 1.0)
}

/// The prescribed Dirichlet data of parameter `param` (0 = `theta_L`,
/// 1 = `theta_W`, 2 = `theta_G`), evaluated from the coordinates of `mesh`.
///
/// # Panics
///
/// Panics if `param > 2`.
pub fn prescribed_data(
    mesh: &TetMesh,
    roles: &TransmonMorphRoles,
    param: usize,
) -> Vec<(u32, [f64; 3])> {
    let s = &roles.spec;
    let p = |n: u32| mesh.nodes[n as usize];
    match param {
        0 => roles
            .island_body
            .iter()
            .map(|&n| (n, [0.0, p(n)[1] - s.y_pin, 0.0]))
            .collect(),
        1 => {
            let x_axis = island_axis(mesh, &roles.island);
            roles
                .island_body
                .iter()
                .map(|&n| {
                    let ramp = ((p(n)[1] - s.y_pin) / s.width_ramp).clamp(0.0, 1.0);
                    (n, [(p(n)[0] - x_axis) * ramp, 0.0, 0.0])
                })
                .collect()
        }
        2 => {
            let x_axis = island_axis(mesh, &roles.island);
            roles
                .gap_edge
                .iter()
                .map(|&n| {
                    let sign = if p(n)[0] > x_axis { 1.0 } else { -1.0 };
                    (
                        n,
                        [sign * roles.gap_scale * gap_taper(p(n)[1], s), 0.0, 0.0],
                    )
                })
                .collect()
        }
        _ => panic!("prescribed_data: parameter index {param} out of range (0..3)"),
    }
}

/// The three harmonic morph fields `[D_L, D_W, D_G]` on `mesh` (one P1-Laplace
/// solve each, [`harmonic_dirichlet_velocity`]). Wrap them with
/// [`super::FreeformBoundaryMorph::from_columns`] for a design gradient.
///
/// # Errors
///
/// [`ElectrostaticError::ShapeMismatch`] if a prescribed node is also in the
/// field's fixed set (which would silently pin it), else propagates the
/// Laplace assembly / solve errors.
pub fn parameter_fields(
    mesh: &TetMesh,
    roles: &TransmonMorphRoles,
) -> Result<Vec<Vec<[f64; 3]>>, ElectrostaticError> {
    (0..3)
        .map(|param| {
            let prescribed = prescribed_data(mesh, roles, param);
            let fixed = roles.fixed_zero(param);
            let fixed_set: BTreeSet<u32> = fixed.iter().copied().collect();
            if let Some((n, _)) = prescribed.iter().find(|(n, _)| fixed_set.contains(n)) {
                return Err(err(format!(
                    "{}: prescribed node {n} is also fixed (fixed wins, the field would be \
                     silently pinned)",
                    PARAM_NAMES[param]
                )));
            }
            harmonic_dirichlet_velocity(mesh, &prescribed, &fixed)
        })
        .collect()
}

/// What [`audit_field`] measured on one parameter field.
#[derive(Clone, Copy, Debug)]
pub struct FieldAudit {
    /// Max `|D|` component over the field's zero set (must be exactly 0).
    pub max_abs_on_zero_set: f64,
    /// Max `|D_z|` over all nodes (must be exactly 0).
    pub max_abs_z: f64,
    /// Max deviation from the prescribed Dirichlet data (must be exactly 0).
    pub prescribed_max_err: f64,
    /// Max in-plane `|D|` over the moving Dirichlet set (must be > 0).
    pub max_on_moving_set: f64,
    /// Max in-plane `|D|` over the free nodes (> 0 ⇒ a genuine extension).
    pub max_on_free: f64,
    /// For `theta_G`: max in-plane `|D|` over the released band (> 0 ⇒ the
    /// band really moves); 0 for the other parameters.
    pub max_on_band: f64,
}

impl FieldAudit {
    /// The exact-zero and exact-recovery conditions, and nonzero motion where
    /// the field must move.
    pub fn is_clean(&self, param: usize) -> bool {
        self.max_abs_on_zero_set == 0.0
            && self.max_abs_z == 0.0
            && self.prescribed_max_err == 0.0
            && self.max_on_moving_set > 0.0
            && self.max_on_free > 0.0
            && (param != 2 || self.max_on_band > 0.0)
    }
}

/// Audit field `param` against the parameterization's contract (exact zeros,
/// planar motion, exact Dirichlet recovery, genuine motion).
pub fn audit_field(
    mesh: &TetMesh,
    roles: &TransmonMorphRoles,
    param: usize,
    field: &[[f64; 3]],
) -> FieldAudit {
    let inplane = |v: &[f64; 3]| (v[0] * v[0] + v[1] * v[1]).sqrt();
    let zero = roles.zero_set(param);
    let max_abs_on_zero_set = zero
        .iter()
        .flat_map(|&n| field[n as usize].map(f64::abs))
        .fold(0.0_f64, f64::max);
    let max_abs_z = field.iter().map(|v| v[2].abs()).fold(0.0_f64, f64::max);
    let prescribed = prescribed_data(mesh, roles, param);
    let prescribed_max_err = prescribed
        .iter()
        .flat_map(|(n, v)| {
            let f = field[*n as usize];
            (0..3).map(move |d| (f[d] - v[d]).abs())
        })
        .fold(0.0_f64, f64::max);
    let max_on_moving_set = roles
        .moving_set(param)
        .iter()
        .map(|&n| inplane(&field[n as usize]))
        .fold(0.0_f64, f64::max);
    let pinned: BTreeSet<u32> = zero
        .iter()
        .chain(roles.moving_set(param))
        .copied()
        .collect();
    let max_on_free = field
        .iter()
        .enumerate()
        .filter(|(i, _)| !pinned.contains(&(*i as u32)))
        .map(|(_, v)| inplane(v))
        .fold(0.0_f64, f64::max);
    let max_on_band = if param == 2 {
        roles
            .gap_band
            .iter()
            .map(|&n| inplane(&field[n as usize]))
            .fold(0.0_f64, f64::max)
    } else {
        0.0
    };
    FieldAudit {
        max_abs_on_zero_set,
        max_abs_z,
        prescribed_max_err,
        max_on_moving_set,
        max_on_free,
        max_on_band,
    }
}

/// The disjoint, exhaustive partition of the ground + feedline nodes used for
/// the partial-charge decomposition of the island excitation. Precedence
/// order (a node goes to the first bucket that matches):
///
/// 1. `feedline` — every feedline node;
/// 2. ground outside the window `|x − x_axis| ≤ charge_window_x`,
///    `y ∈ charge_window_y` — `rest_of_ground`;
/// 3. `junction_end` — window ground with `y < y_pin + junction_end_dy`;
/// 4. `cutout_sides` — window ground with `y ≤ claw_y`;
/// 5. `cutout_top_border` — window ground above `claw_y` within
///    `0.1·gap_scale` of the cutout (`|x − x_axis| ≤ x_edge + 0.1·gap_scale`,
///    `y ≤ y_top + 0.1·gap_scale`, with the cutout top `y_top` taken as the
///    island top plus `gap_scale`, true on the fixture where the top and side
///    gaps are both 30 μm). On the fixture this is the 2 μm ground strip
///    between the cutout and the claw;
/// 6. `claw_region` — the remaining window ground above `claw_y` (the claw
///    and the start of the resonator).
///
/// The buckets are returned in the order `feedline`, `junction_end`,
/// `cutout_sides`, `cutout_top_border`, `claw_region`, `rest_of_ground`.
/// Feed them to
/// [`super::CapacitanceMatrixShapeGradient::reaction_partition`], which
/// re-checks disjointness and exhaustiveness.
pub fn partial_charge_regions(
    mesh: &TetMesh,
    roles: &TransmonMorphRoles,
) -> Vec<(String, Vec<u32>)> {
    let s = &roles.spec;
    let x_axis = island_axis(mesh, &roles.island);
    let strip_pad = 0.1 * roles.gap_scale;
    let island_top = roles
        .island
        .iter()
        .map(|&n| mesh.nodes[n as usize][1])
        .fold(f64::NEG_INFINITY, f64::max);
    let cutout_top = island_top + roles.gap_scale;
    let mut buckets: Vec<(String, Vec<u32>)> = [
        "feedline",
        "junction_end",
        "cutout_sides",
        "cutout_top_border",
        "claw_region",
        "rest_of_ground",
    ]
    .iter()
    .map(|n| ((*n).to_string(), Vec::new()))
    .collect();
    buckets[0].1 = roles.feedline.clone();
    for &n in &roles.ground {
        let q = mesh.nodes[n as usize];
        let ax = (q[0] - x_axis).abs();
        let in_window =
            ax <= s.charge_window_x && q[1] >= s.charge_window_y.0 && q[1] <= s.charge_window_y.1;
        let b = if !in_window {
            5
        } else if q[1] < s.y_pin + s.junction_end_dy {
            1
        } else if q[1] <= s.claw_y {
            2
        } else if ax <= roles.x_edge + strip_pad && q[1] <= cutout_top + strip_pad {
            3
        } else {
            4
        };
        buckets[b].1.push(n);
    }
    buckets
}

/// A one-sided mesh-validity budget along a node-motion field.
#[derive(Clone, Copy, Debug)]
pub struct DirectionalBudget {
    /// The bound: the largest `|θ|` in the searched sign whose worst tet
    /// still meets the ratio floor (signed).
    pub theta: f64,
    /// `true` if the floor was never violated within `|θ| ≤ max_abs_theta`
    /// (then `theta` is the search limit, not a budget).
    pub unbounded: bool,
    /// The worst tet's volume ratio at `theta`.
    pub ratio: f64,
    /// Base-mesh centroid of the worst tet at `theta`.
    pub worst_centroid: [f64; 3],
}

/// Bisect the mesh-validity budget along `vel` in the sign of `sign` (±1):
/// the largest `|θ|` with `min_tet_volume_ratio ≥ ratio_floor`. The bracket
/// is found by geometric expansion from `|θ| = 0.01` (×1.5), capped at
/// `max_abs_theta`; a direction that never reaches the floor within the cap
/// is reported as [`DirectionalBudget::unbounded`].
///
/// # Panics
///
/// Panics if `sign` is zero or `vel.len() != base.n_nodes()`.
pub fn bisect_budget(
    base: &TetMesh,
    vel: &[[f64; 3]],
    ratio_floor: f64,
    sign: f64,
    max_abs_theta: f64,
) -> DirectionalBudget {
    assert!(sign != 0.0, "bisect_budget: sign must be nonzero");
    let sgn = sign.signum();
    let ratio_at = |th: f64| min_tet_volume_ratio(base, &super::apply_node_motion(base, vel, th));
    let mut bad = 0.01_f64;
    let mut unbounded = false;
    while ratio_at(sgn * bad) >= ratio_floor {
        if bad >= max_abs_theta {
            unbounded = true;
            break;
        }
        bad = (bad * 1.5).min(max_abs_theta);
    }
    let mut good = 0.0_f64;
    if unbounded {
        good = bad;
    } else {
        for _ in 0..60 {
            let mid = 0.5 * (good + bad);
            if ratio_at(sgn * mid) >= ratio_floor {
                good = mid;
            } else {
                bad = mid;
            }
        }
    }
    let theta = sgn * good;
    let moved = super::apply_node_motion(base, vel, theta);
    let (ratio, t) = worst_tet_volume_ratio(base, &moved);
    let tet = base.tets[t];
    let mut c = [0.0_f64; 3];
    for &v in &tet {
        for (cd, x) in c.iter_mut().zip(base.nodes[v as usize]) {
            *cd += 0.25 * x;
        }
    }
    DirectionalBudget {
        theta,
        unbounded,
        ratio,
        worst_centroid: c,
    }
}
