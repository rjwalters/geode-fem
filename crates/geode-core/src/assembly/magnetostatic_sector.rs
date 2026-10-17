//! **Sector symmetry and rotation** for 2-D nonlinear magnetostatics:
//! periodic / antiperiodic node pairing of the two radial edges of a machine
//! sector, and a **conforming sliding band** that rotates the rotor by whole
//! coupling-circle steps (motor-paper milestone M2, issue #1161).
//!
//! Both are expressed as signed node links on
//! [`NonlinearMagnetostaticSystem`]
//! ([`NonlinearMagnetostaticSystem::with_node_links`]), so the reduced
//! residual is `Pᵀ R(P a)`, the reduced tangent is the symmetric `Pᵀ K P`,
//! and the M3 adjoint transposes the same `P`
//! ([`NonlinearMagnetostaticSolution::prolongation`]).
//!
//! # Sector edges
//!
//! A machine with `2p` poles solved on one pole pitch `α = π/p` has
//! `A(R_α x) = −A(x)` (antiperiodic). Two pole pitches give `+A`
//! (periodic). [`sector_links`] rotates every node of the low radial edge by
//! `α` and pairs it with the high-edge node it lands on:
//! `A_high = s · A_low` with `s = ` [`Periodicity::sign`]. The match is a
//! bijection within a tolerance, and anything else fails loudly
//! ([`SectorError::UnmatchedNode`], [`SectorError::InvalidInput`]): there is
//! no snapping and no approximate coupling. A node that is its own image (the
//! centre of a sector that contains the axis) is paired with itself; for an
//! antiperiodic sector that forces `A = 0` there, which is the exact
//! symmetry value.
//!
//! # Sliding band
//!
//! The rotor and stator meshes meet on a circle of radius `r_c` with `N`
//! uniformly spaced nodes over the sector on **each** side, at angles
//! `θ0 + iΔ`, `Δ = α/N`, `i = 0…N`. Rotating the rotor by `kΔ` moves rotor
//! node `i` onto the position of stator node `i + k`. That is an exact
//! re-connection with no remeshing:
//!
//! ```text
//!   A_rotor[i] = s^w · A_stator[m],   i + k = w N + m,  0 ≤ m < N ,
//! ```
//!
//! where the wrap count `w` (negative for `k < 0`) picks up the sector sign.
//! The rotor node coordinates are rotated physically by `kΔ`
//! ([`SlidingBand::rotated_mesh`]) and so is the per-triangle rotor
//! magnetization ([`SlidingBand::rotate_rotor_vectors`]), so `B` in the
//! rotor and magnets is in the lab frame. Rotor triangles that end up past
//! the sector edge are the antiperiodic images of the part that left; the
//! Arkkio integrand `B_r B_θ` is invariant under that image, and
//! [`SlidingBandSolution::sample_b`] maps lab points into the mesh through it.
//!
//! **Angle grid.** Only `θ = kΔ`, `k ∈ ℤ`, is reachable; arbitrary angles in
//! between are out of scope. With the reference coupling `N` a multiple of
//! 60 over a 60° sector, `Δ = 60°/N` divides 1°, so the 1° and 2°
//! mechanical steps of the reference are `k = N/60` and `2N/60`.
//! [`SlidingBand::index_of_angle`] converts and fails loudly off the grid.
//!
//! # Harmonic coupling
//!
//! [`CouplingMode::Harmonic`] replaces the nodal re-connection by weak
//! continuity of a chosen set of angular harmonics of the trace on the
//! coupling circle (issue #1170), the coupling model of the ICEM reference
//! code (`boundaries.coupling_sine_and_cosine_harmonics` in
//! `benchmarks/motor_reference/conventions.json`). The two sides keep their
//! own coupling nodes and are joined only through the constraints
//!
//! ```text
//!   ∫ A_stator(θ) e(θ) r_c dθ − ∫ A_rotor(θ) e(θ) r_c dθ = 0 ,
//!   e ∈ { sin nθ, cos nθ : n in the chosen orders } ,
//! ```
//!
//! imposed with Lagrange multipliers by
//! [`NonlinearMagnetostaticSystem::solve_constrained`]. Both traces are the
//! P1 traces in the lab frame, piecewise linear in `θ` between coupling
//! nodes, so the moment of a hat function centred at `θ_i` is exactly
//! `r_c Δ sinc²(nΔ/2) · e(θ_i)`; the rotor hats sit at the rotated angles
//! `θ0 + (i + k)Δ`. For a sector the orders must have the sector symmetry
//! (`sin(n(θ + α)) = s · sin nθ`; antiperiodic over 60° gives
//! `n = 3, 9, 15, …`), so the product of trace and test function is
//! `α`-periodic and the moment over the rotor's rotated arc equals the
//! moment over the sector. Modes outside the chosen set are not transmitted:
//! on each side their normal flux is zero (the natural condition).
//!
//! [`CouplingMode::HarmonicFreeEnds`] also leaves the two end nodes of each
//! side's coupling trace out of the radial-edge pairs, as the reference does
//! (its antiperiodic DOF sets exclude the coupling DOFs); those nodes then
//! carry their own half hats over the side's own arc, and the trace is
//! antiperiodic only in the constrained modes.
//!
//! With every symmetric order below the grid's Nyquist limit (`nΔ < π`),
//! the constraints span the whole antiperiodic trace space, and the solution
//! equals the conforming one; `tests/magnetostatic_sector_harmonic.rs` checks
//! this.
//!
//! **Coupling torque.** The rotor energy is invariant under the rigid
//! rotation, so the angle enters the constrained minimum only through the
//! constraints. The envelope theorem gives the torque on the rotor in the
//! `+θ` direction per unit axial length,
//! `T = −dΠ*/dβ = −λᵀ ∂(Gᵀ A)/∂β`, with `∂/∂β` acting on the rotor hat
//! centres ([`SlidingBand::harmonic_coupling_torque`]). This is the
//! `calcTorqueCoupling` construction of the reference, re-derived here.
//!
//! # Mesh input
//!
//! [`SlidingBand::new`] takes the mesh, the per-triangle region tags split
//! into rotor and stator tag sets (the inner and outer air-gap bands go to
//! the rotor and stator side respectively), and the node sets of the
//! coupling circle and the two radial edges, which
//! [`SectorBoundaryNodes::from_tagged_edges`] collects from tagged boundary
//! edges (gmsh physical curves). The coupling-circle nodes may be shared by
//! the two sides (a conforming mesh at angle 0: they are split here) or
//! already duplicated. A node shared by rotor and stator triangles anywhere
//! else is an error.

use std::collections::HashSet;
use std::f64::consts::TAU;

use crate::analytic::waveguide::TriMesh;
use crate::assembly::magnetostatic::recover_b_field;
use crate::assembly::magnetostatic_nonlinear::{
    LinearConstraints, MaterialMap, NewtonConfig, NodeLink, NonlinearMagnetostaticError,
    NonlinearMagnetostaticSolution, NonlinearMagnetostaticSystem,
};
use crate::assembly::torque::locate_triangle;

/// Default relative node-matching tolerance (times the largest radius).
pub const MATCH_TOL_REL_DEFAULT: f64 = 1e-8;

// ─────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────

/// Error from sector pairing, the sliding band, or the solve under them.
#[derive(Debug, Clone, PartialEq)]
pub enum SectorError {
    /// Malformed input: length mismatches, tags, angles, empty or unequal
    /// node sets, a node shared by rotor and stator off the coupling circle.
    InvalidInput(String),
    /// A node has no partner within the tolerance (or its nearest partner
    /// is already taken).
    UnmatchedNode {
        /// Which node set (for example `"rotor radial edge"`).
        set: String,
        /// The node with no partner.
        node: u32,
        /// Its coordinates.
        position: [f64; 2],
        /// The image point it was matched against.
        image: [f64; 2],
        /// The distance to the nearest candidate partner (∞ if none).
        nearest_distance: f64,
        /// The matching tolerance.
        tolerance: f64,
    },
    /// The coupling-circle nodes are not on one circle, not uniformly
    /// spaced, or not the same on the two sides.
    NonUniformCoupling(String),
    /// A requested angle is not a whole number of coupling steps.
    OffGrid {
        /// The requested angle (rad).
        angle: f64,
        /// The coupling step `Δ` (rad).
        step: f64,
    },
    /// The nonlinear solve (or the system set-up) failed.
    Solve(NonlinearMagnetostaticError),
}

impl std::fmt::Display for SectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(s) => write!(f, "sector constraint input: {s}"),
            Self::UnmatchedNode {
                set,
                node,
                position,
                image,
                nearest_distance,
                tolerance,
            } => write!(
                f,
                "{set}: node {node} at ({:.9e}, {:.9e}) has no partner at its image \
                 ({:.9e}, {:.9e}): nearest free candidate is {nearest_distance:.3e} away, \
                 tolerance {tolerance:.3e}. The two edges must be meshed as exact rotations \
                 of each other (gmsh `Periodic Curve`)",
                position[0], position[1], image[0], image[1]
            ),
            Self::NonUniformCoupling(s) => write!(f, "sliding-band coupling circle: {s}"),
            Self::OffGrid { angle, step } => write!(
                f,
                "angle {angle:e} rad is not a whole number of coupling steps {step:e} rad"
            ),
            Self::Solve(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SectorError {}

impl From<NonlinearMagnetostaticError> for SectorError {
    fn from(e: NonlinearMagnetostaticError) -> Self {
        Self::Solve(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Sector pairing
// ─────────────────────────────────────────────────────────────────────────

/// The symmetry of the two sector edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Periodicity {
    /// `A(R_α x) = +A(x)` (an even number of pole pitches).
    Periodic,
    /// `A(R_α x) = −A(x)` (an odd number of pole pitches).
    Antiperiodic,
}

impl Periodicity {
    /// `+1` or `−1`.
    pub fn sign(self) -> f64 {
        match self {
            Self::Periodic => 1.0,
            Self::Antiperiodic => -1.0,
        }
    }

    /// `sign^w` for an integer wrap count `w` (any sign).
    pub fn sign_pow(self, w: i64) -> f64 {
        if self == Self::Antiperiodic && w.rem_euclid(2) == 1 {
            -1.0
        } else {
            1.0
        }
    }
}

/// Result of a node match: the pairs and the worst distance.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeMatch {
    /// `(low node, high node)` pairs.
    pub pairs: Vec<(u32, u32)>,
    /// Largest distance between a rotated low node and its partner.
    pub max_distance: f64,
    /// The tolerance used.
    pub tolerance: f64,
}

fn rot(p: [f64; 2], c: f64, s: f64) -> [f64; 2] {
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn sorted_unique(v: &[u32]) -> Vec<u32> {
    let mut v = v.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

/// Pair every node of `low` with the node of `high` at its image under the
/// rotation by `angle` about the origin. The result is a bijection, with
/// every distance at most `tolerance`.
///
/// # Errors
///
/// [`SectorError::InvalidInput`] if the sets are empty, differ in size, or
/// hold an out-of-range index; [`SectorError::UnmatchedNode`] for a low
/// node with no free high node within `tolerance`.
pub fn match_rotated_nodes(
    nodes: &[[f64; 2]],
    low: &[u32],
    high: &[u32],
    angle: f64,
    tolerance: f64,
    set: &str,
) -> Result<NodeMatch, SectorError> {
    let low = sorted_unique(low);
    let high = sorted_unique(high);
    if low.is_empty() || low.len() != high.len() {
        return Err(SectorError::InvalidInput(format!(
            "{set}: low edge has {} nodes and high edge {}; they must be equal and non-zero",
            low.len(),
            high.len()
        )));
    }
    if let Some(&bad) = low
        .iter()
        .chain(&high)
        .find(|&&n| n as usize >= nodes.len())
    {
        return Err(SectorError::InvalidInput(format!(
            "{set}: node {bad} out of range for {} nodes",
            nodes.len()
        )));
    }
    if !(tolerance.is_finite() && tolerance > 0.0) {
        return Err(SectorError::InvalidInput(format!(
            "{set}: tolerance must be finite and > 0, got {tolerance}"
        )));
    }
    let (c, s) = (angle.cos(), angle.sin());
    let mut taken = vec![false; high.len()];
    let mut pairs = Vec::with_capacity(low.len());
    let mut max_distance = 0.0_f64;
    for &l in &low {
        let p = nodes[l as usize];
        let img = rot(p, c, s);
        let mut best = (f64::INFINITY, usize::MAX);
        for (hi, &h) in high.iter().enumerate() {
            if taken[hi] {
                continue;
            }
            let d = dist(img, nodes[h as usize]);
            if d < best.0 {
                best = (d, hi);
            }
        }
        if best.0 > tolerance {
            return Err(SectorError::UnmatchedNode {
                set: set.to_string(),
                node: l,
                position: p,
                image: img,
                nearest_distance: best.0,
                tolerance,
            });
        }
        taken[best.1] = true;
        max_distance = max_distance.max(best.0);
        pairs.push((l, high[best.1]));
    }
    Ok(NodeMatch {
        pairs,
        max_distance,
        tolerance,
    })
}

/// Node links `A_high = s · A_low` for the two radial edges of a sector of
/// angle `sector_angle` (see the module docs), and the match report. Pass
/// the links to [`NonlinearMagnetostaticSystem::with_node_links`].
///
/// `tol_rel` is relative to the largest edge-node radius
/// ([`MATCH_TOL_REL_DEFAULT`] is a good default for exactly periodic
/// meshes).
///
/// # Errors
///
/// As [`match_rotated_nodes`].
pub fn sector_links(
    mesh: &TriMesh,
    low: &[u32],
    high: &[u32],
    sector_angle: f64,
    periodicity: Periodicity,
    tol_rel: f64,
) -> Result<(Vec<NodeLink>, NodeMatch), SectorError> {
    let scale = low
        .iter()
        .chain(high)
        .filter_map(|&n| mesh.nodes.get(n as usize))
        .map(|p| p[0].hypot(p[1]))
        .fold(0.0_f64, f64::max);
    let tol = tol_rel * scale.max(f64::MIN_POSITIVE);
    let m = match_rotated_nodes(
        &mesh.nodes,
        low,
        high,
        sector_angle,
        tol,
        "sector radial edge",
    )?;
    let links = m
        .pairs
        .iter()
        .map(|&(l, h)| NodeLink::new(h as usize, l as usize, periodicity.sign()))
        .collect();
    Ok((links, m))
}

// ─────────────────────────────────────────────────────────────────────────
// Sliding band
// ─────────────────────────────────────────────────────────────────────────

/// The node sets of a sector boundary (indices into the input mesh).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectorBoundaryNodes {
    /// Nodes on the coupling circle (one or both sides).
    pub coupling: Vec<u32>,
    /// Nodes on the radial edge at `θ = θ0` (rotor and stator parts).
    pub low_edge: Vec<u32>,
    /// Nodes on the radial edge at `θ = θ0 + α` (rotor and stator parts).
    pub high_edge: Vec<u32>,
}

impl SectorBoundaryNodes {
    /// Collect the node sets from tagged boundary edges (gmsh physical
    /// curves): every endpoint of an edge whose tag is in the given list.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] if `edges` and `edge_tags` differ in
    /// length.
    pub fn from_tagged_edges(
        edges: &[[u32; 2]],
        edge_tags: &[i32],
        coupling_tags: &[i32],
        low_tags: &[i32],
        high_tags: &[i32],
    ) -> Result<Self, SectorError> {
        if edges.len() != edge_tags.len() {
            return Err(SectorError::InvalidInput(format!(
                "{} edges but {} edge tags",
                edges.len(),
                edge_tags.len()
            )));
        }
        let collect = |tags: &[i32]| {
            let v: Vec<u32> = edges
                .iter()
                .zip(edge_tags)
                .filter(|(_, t)| tags.contains(t))
                .flat_map(|(e, _)| [e[0], e[1]])
                .collect();
            sorted_unique(&v)
        };
        Ok(Self {
            coupling: collect(coupling_tags),
            low_edge: collect(low_tags),
            high_edge: collect(high_tags),
        })
    }
}

/// Configuration of a [`SlidingBand`].
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingBandSpec {
    /// Sector angle `α` (rad); `2π` for a full machine (then the radial
    /// edge sets must be empty).
    pub sector_angle: f64,
    /// Symmetry of the sector edges (ignored for the full machine).
    pub periodicity: Periodicity,
    /// Triangle tags of the rotor side (rotor iron, magnets, rotor air, the
    /// inner air-gap band).
    pub rotor_tags: Vec<i32>,
    /// Triangle tags of the stator side (the outer air-gap band, stator
    /// iron, windings, slot air).
    pub stator_tags: Vec<i32>,
    /// Relative node-matching tolerance, times the largest coupling or edge
    /// radius.
    pub match_tol_rel: f64,
}

/// What [`SlidingBand::new`] found in the mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingBandReport {
    /// Coupling nodes per side over the sector, `N`.
    pub n_coupling: usize,
    /// Coupling radius `r_c` (mean of the coupling nodes).
    pub r_c: f64,
    /// Largest deviation of a coupling node from `r_c`.
    pub max_radius_deviation: f64,
    /// Largest deviation of a coupling node from its grid angle, as an arc
    /// length.
    pub max_spacing_deviation: f64,
    /// Coupling nodes that were shared by both sides and split here.
    pub n_split: usize,
    /// Clockwise input triangles that were relabelled counter-clockwise.
    /// This is a canonicalisation only: every per-element term the band
    /// solve uses (stiffness, tangent, current and magnetization loads, `B`
    /// recovery, Arkkio torque) is orientation-independent (#1166), so the
    /// solution does not depend on it.
    pub n_reoriented: usize,
    /// Rotor radial-edge pairs.
    pub n_rotor_pairs: usize,
    /// Stator radial-edge pairs.
    pub n_stator_pairs: usize,
    /// Largest radial-edge match distance.
    pub max_match_distance: f64,
    /// The matching tolerance (absolute).
    pub tolerance: f64,
}

/// A conforming sliding band over a machine sector (see the module docs).
///
/// Build it once with [`new`](Self::new); then [`links`](Self::links) and
/// [`rotated_mesh`](Self::rotated_mesh) give the constraints and geometry
/// at any step `k`, and a [`SlidingBandProblem`] solves there.
#[derive(Debug, Clone)]
pub struct SlidingBand {
    /// The mesh with the coupling circle split (reference position `k = 0`).
    mesh: TriMesh,
    tri_tags: Vec<i32>,
    /// Original node of each node (identity except for split copies).
    original: Vec<u32>,
    n_original: usize,
    rotor_node: Vec<bool>,
    rotor_tri: Vec<bool>,
    /// Coupling nodes by grid index (`N + 1` for a sector, `N` for the full
    /// circle).
    rotor_coupling: Vec<u32>,
    stator_coupling: Vec<u32>,
    n: usize,
    full: bool,
    sector_angle: f64,
    /// Lab angle of coupling grid index 0 at `k = 0`.
    theta0: f64,
    r_c: f64,
    periodicity: Periodicity,
    fixed_links: Vec<NodeLink>,
    report: SlidingBandReport,
}

impl SlidingBand {
    /// Analyse the mesh and build the band.
    ///
    /// # Errors
    ///
    /// * [`SectorError::InvalidInput`]: tag-length mismatch, a triangle tag
    ///   in neither or both tag sets, a sector angle outside `(0, 2π]`,
    ///   radial edges given for the full machine (or missing for a sector),
    ///   a coupling node used by no triangle, a node shared by rotor and
    ///   stator off the coupling circle, a radial edge that is not a ray, or
    ///   a coupling end node missing from the radial edges.
    /// * [`SectorError::NonUniformCoupling`]: coupling nodes off one circle,
    ///   off the uniform grid, or different on the two sides.
    /// * [`SectorError::UnmatchedNode`]: the radial edges do not match.
    pub fn new(
        mesh: &TriMesh,
        tri_tags: &[i32],
        nodes: &SectorBoundaryNodes,
        spec: &SlidingBandSpec,
    ) -> Result<Self, SectorError> {
        let bad = |s: String| Err(SectorError::InvalidInput(s));
        if tri_tags.len() != mesh.n_tris() {
            return bad(format!(
                "{} triangle tags for {} triangles",
                tri_tags.len(),
                mesh.n_tris()
            ));
        }
        let full = (spec.sector_angle - TAU).abs() <= 1e-12;
        if !(full || (spec.sector_angle > 0.0 && spec.sector_angle < TAU)) {
            return bad(format!(
                "sector angle must be in (0, 2pi]; got {}",
                spec.sector_angle
            ));
        }
        if full != (nodes.low_edge.is_empty() && nodes.high_edge.is_empty()) {
            return bad(if full {
                "a full machine (sector angle 2pi) has no radial edges".into()
            } else {
                "a sector needs both radial edge node sets".into()
            });
        }
        if !(spec.match_tol_rel.is_finite() && spec.match_tol_rel > 0.0) {
            return bad(format!(
                "match_tol_rel must be finite and > 0; got {}",
                spec.match_tol_rel
            ));
        }
        let n0 = mesh.n_nodes();
        if let Some(&b) = nodes
            .coupling
            .iter()
            .chain(&nodes.low_edge)
            .chain(&nodes.high_edge)
            .find(|&&i| i as usize >= n0)
        {
            return bad(format!("boundary node {b} out of range for {n0} nodes"));
        }

        // Classify triangles.
        let rotor_set: HashSet<i32> = spec.rotor_tags.iter().copied().collect();
        let stator_set: HashSet<i32> = spec.stator_tags.iter().copied().collect();
        let mut rotor_tri = Vec::with_capacity(mesh.n_tris());
        for (t, &tag) in tri_tags.iter().enumerate() {
            match (rotor_set.contains(&tag), stator_set.contains(&tag)) {
                (true, false) => rotor_tri.push(true),
                (false, true) => rotor_tri.push(false),
                (true, true) => {
                    return bad(format!(
                        "tag {tag} is in both the rotor and stator tag sets"
                    ));
                }
                (false, false) => {
                    return bad(format!(
                        "triangle {t} has tag {tag}, which is in neither the rotor nor the \
                         stator tag set"
                    ));
                }
            }
        }
        let mut used_r = vec![false; n0];
        let mut used_s = vec![false; n0];
        for (tri, &r) in mesh.tris.iter().zip(&rotor_tri) {
            for &p in tri {
                if r {
                    used_r[p as usize] = true;
                } else {
                    used_s[p as usize] = true;
                }
            }
        }

        // Split shared coupling nodes.
        let coupling = sorted_unique(&nodes.coupling);
        if coupling.is_empty() {
            return bad("the coupling-circle node set is empty".into());
        }
        let coupling_set: HashSet<u32> = coupling.iter().copied().collect();
        for i in 0..n0 {
            if used_r[i] && used_s[i] && !coupling_set.contains(&(i as u32)) {
                return bad(format!(
                    "node {i} at ({:e}, {:e}) is shared by rotor and stator triangles but is \
                     not on the coupling circle",
                    mesh.nodes[i][0], mesh.nodes[i][1]
                ));
            }
        }
        let mut nodes_out = mesh.nodes.clone();
        let mut original: Vec<u32> = (0..n0 as u32).collect();
        let mut rotor_copy: Vec<Option<u32>> = vec![None; n0];
        let mut rotor_c = Vec::new();
        let mut stator_c = Vec::new();
        let mut n_split = 0;
        for &c in &coupling {
            let ci = c as usize;
            match (used_r[ci], used_s[ci]) {
                (true, true) => {
                    let new = nodes_out.len() as u32;
                    nodes_out.push(mesh.nodes[ci]);
                    original.push(c);
                    rotor_copy[ci] = Some(new);
                    rotor_c.push(new);
                    stator_c.push(c);
                    n_split += 1;
                }
                (true, false) => rotor_c.push(c),
                (false, true) => stator_c.push(c),
                (false, false) => {
                    return bad(format!("coupling node {c} is used by no triangle"));
                }
            }
        }
        let mut tris = mesh.tris.clone();
        // Canonicalise every triangle to counter-clockwise (a pure relabelling
        // within the triangle). The assemblers no longer need it (#1166: all
        // per-element terms use |T| and true gradients), but a consistent
        // orientation keeps the band's reported mesh canonical.
        let mut n_reoriented = 0usize;
        for tri in tris.iter_mut() {
            let [a, b, c] = tri.map(|i| mesh.nodes[i as usize]);
            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if cross < 0.0 {
                tri.swap(1, 2);
                n_reoriented += 1;
            }
        }
        for (tri, &r) in tris.iter_mut().zip(&rotor_tri) {
            if r {
                for p in tri.iter_mut() {
                    if let Some(new) = rotor_copy[*p as usize] {
                        *p = new;
                    }
                }
            }
        }
        let n_all = nodes_out.len();
        let mut rotor_node = vec![false; n_all];
        for (tri, &r) in tris.iter().zip(&rotor_tri) {
            if r {
                for &p in tri {
                    rotor_node[p as usize] = true;
                }
            }
        }
        let split_mesh = TriMesh {
            nodes: nodes_out,
            tris,
        };

        // Coupling circle: radius, angles, uniform grid.
        let scale = rotor_c
            .iter()
            .chain(&stator_c)
            .chain(&nodes.low_edge)
            .chain(&nodes.high_edge)
            .map(|&n| {
                let p = split_mesh.nodes[n as usize];
                p[0].hypot(p[1])
            })
            .fold(0.0_f64, f64::max);
        let tol = spec.match_tol_rel * scale.max(f64::MIN_POSITIVE);
        let radius = |n: u32| {
            let p = split_mesh.nodes[n as usize];
            p[0].hypot(p[1])
        };
        let all_c: Vec<u32> = rotor_c.iter().chain(&stator_c).copied().collect();
        let r_c = all_c.iter().map(|&n| radius(n)).sum::<f64>() / all_c.len() as f64;
        let max_radius_deviation = all_c
            .iter()
            .map(|&n| (radius(n) - r_c).abs())
            .fold(0.0_f64, f64::max);
        if max_radius_deviation > tol {
            return Err(SectorError::NonUniformCoupling(format!(
                "coupling nodes are not on one circle: radius {r_c:e} +- {max_radius_deviation:e} \
                 exceeds tolerance {tol:e}"
            )));
        }
        if rotor_c.len() != stator_c.len() {
            return Err(SectorError::NonUniformCoupling(format!(
                "rotor side has {} coupling nodes and stator side {}",
                rotor_c.len(),
                stator_c.len()
            )));
        }

        // Sector start angle θ0 from the low radial edge (0 for the full
        // circle).
        let theta0 = if full {
            0.0
        } else {
            let far = *nodes
                .low_edge
                .iter()
                .max_by(|&&a, &&b| radius(a).total_cmp(&radius(b)))
                .expect("non-empty low edge");
            let p = split_mesh.nodes[far as usize];
            let th = p[1].atan2(p[0]);
            let u = [th.cos(), th.sin()];
            for &n in &nodes.low_edge {
                let q = split_mesh.nodes[n as usize];
                let off = (u[0] * q[1] - u[1] * q[0]).abs();
                if off > tol || u[0] * q[0] + u[1] * q[1] < -tol {
                    return bad(format!(
                        "low radial edge is not a ray from the origin: node {n} is {off:e} \
                         off the ray at angle {th}"
                    ));
                }
            }
            th
        };
        let n = if full {
            rotor_c.len()
        } else {
            rotor_c.len() - 1
        };
        if n < 1 || (full && n < 3) {
            return Err(SectorError::NonUniformCoupling(format!(
                "too few coupling nodes ({})",
                rotor_c.len()
            )));
        }
        let step = spec.sector_angle / n as f64;
        let mut max_spacing_deviation = 0.0_f64;
        let mut index_side = |side: &[u32], name: &str| -> Result<Vec<u32>, SectorError> {
            let slots = if full { n } else { n + 1 };
            let mut by_index = vec![u32::MAX; slots];
            let (c0, s0) = ((-theta0).cos(), (-theta0).sin());
            for &node in side {
                let q = rot(split_mesh.nodes[node as usize], c0, s0);
                let mut phi = q[1].atan2(q[0]);
                if phi < -0.5 * step {
                    phi += TAU;
                }
                let idx = (phi / step).round();
                let dev = (phi - idx * step).abs() * r_c;
                let in_range = idx >= 0.0 && idx <= n as f64;
                if dev > tol || !in_range {
                    return Err(SectorError::NonUniformCoupling(format!(
                        "{name} coupling node {node} at angle {:.12} rad (from theta0) is off \
                         the uniform grid of step {step:.12} rad: deviation {dev:e} m, \
                         tolerance {tol:e} m",
                        phi
                    )));
                }
                max_spacing_deviation = max_spacing_deviation.max(dev);
                let mut idx = idx as usize;
                if full && idx == n {
                    idx = 0;
                }
                if by_index[idx] != u32::MAX {
                    return Err(SectorError::NonUniformCoupling(format!(
                        "{name} coupling nodes {} and {node} share grid index {idx}",
                        by_index[idx]
                    )));
                }
                by_index[idx] = node;
            }
            if let Some(missing) = by_index.iter().position(|&x| x == u32::MAX) {
                return Err(SectorError::NonUniformCoupling(format!(
                    "{name} coupling circle has no node at grid index {missing}"
                )));
            }
            Ok(by_index)
        };
        let rotor_coupling = index_side(&rotor_c, "rotor")?;
        let stator_coupling = index_side(&stator_c, "stator")?;

        // Radial edges, matched within each side.
        let mut fixed_links = Vec::new();
        let (mut n_rotor_pairs, mut n_stator_pairs, mut max_match_distance) = (0, 0, 0.0_f64);
        if !full {
            let expand = |set: &[u32]| -> Vec<u32> {
                let mut v = Vec::with_capacity(set.len() + 2);
                for &x in set {
                    v.push(x);
                    if let Some(c) = rotor_copy[x as usize] {
                        v.push(c);
                    }
                }
                sorted_unique(&v)
            };
            let low = expand(&nodes.low_edge);
            let high = expand(&nodes.high_edge);
            for (side_is_rotor, name) in
                [(true, "rotor radial edge"), (false, "stator radial edge")]
            {
                let pick = |v: &[u32]| -> Vec<u32> {
                    v.iter()
                        .copied()
                        .filter(|&x| rotor_node[x as usize] == side_is_rotor)
                        .collect()
                };
                let (l, h) = (pick(&low), pick(&high));
                let m =
                    match_rotated_nodes(&split_mesh.nodes, &l, &h, spec.sector_angle, tol, name)?;
                max_match_distance = max_match_distance.max(m.max_distance);
                if side_is_rotor {
                    n_rotor_pairs = m.pairs.len();
                } else {
                    n_stator_pairs = m.pairs.len();
                }
                let (lset, hset): (HashSet<u32>, HashSet<u32>) =
                    (l.iter().copied().collect(), h.iter().copied().collect());
                let ends = if side_is_rotor {
                    (&rotor_coupling[0], &rotor_coupling[n])
                } else {
                    (&stator_coupling[0], &stator_coupling[n])
                };
                if !lset.contains(ends.0) || !hset.contains(ends.1) {
                    return bad(format!(
                        "{name}: the coupling-circle end nodes {} and {} must lie on the low \
                         and high radial edges",
                        ends.0, ends.1
                    ));
                }
                for (lo, hi) in m.pairs {
                    fixed_links.push(NodeLink::new(
                        hi as usize,
                        lo as usize,
                        spec.periodicity.sign(),
                    ));
                }
            }
        }

        Ok(Self {
            mesh: split_mesh,
            tri_tags: tri_tags.to_vec(),
            original,
            n_original: n0,
            rotor_node,
            rotor_tri,
            rotor_coupling,
            stator_coupling,
            n,
            full,
            sector_angle: spec.sector_angle,
            theta0,
            r_c,
            periodicity: spec.periodicity,
            fixed_links,
            report: SlidingBandReport {
                n_coupling: n,
                r_c,
                max_radius_deviation,
                max_spacing_deviation,
                n_split,
                n_reoriented,
                n_rotor_pairs,
                n_stator_pairs,
                max_match_distance,
                tolerance: tol,
            },
        })
    }

    /// What the analysis found.
    pub fn report(&self) -> &SlidingBandReport {
        &self.report
    }

    /// The split mesh at the reference position `k = 0`.
    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// Per-triangle region tags (unchanged by the split).
    pub fn tri_tags(&self) -> &[i32] {
        &self.tri_tags
    }

    /// Node count of the split mesh.
    pub fn n_nodes(&self) -> usize {
        self.mesh.n_nodes()
    }

    /// Input-mesh node of each split-mesh node (split copies map to the
    /// node they were copied from).
    pub fn original_nodes(&self) -> &[u32] {
        &self.original
    }

    /// Whether each split-mesh node belongs to the rotor.
    pub fn rotor_nodes(&self) -> &[bool] {
        &self.rotor_node
    }

    /// Whether each triangle belongs to the rotor.
    pub fn rotor_triangles(&self) -> &[bool] {
        &self.rotor_tri
    }

    /// Coupling nodes per side over the sector, `N`.
    pub fn n_coupling(&self) -> usize {
        self.n
    }

    /// The coupling step `Δ = α/N` (rad), the rotation-angle grid.
    pub fn step_angle(&self) -> f64 {
        self.sector_angle / self.n as f64
    }

    /// The sector angle `α` (rad).
    pub fn sector_angle(&self) -> f64 {
        self.sector_angle
    }

    /// The sector symmetry.
    pub fn periodicity(&self) -> Periodicity {
        self.periodicity
    }

    /// Rotor angle of step `k`, `kΔ` (rad).
    pub fn angle_of(&self, k: i64) -> f64 {
        k as f64 * self.step_angle()
    }

    /// The step `k` with `kΔ = angle`.
    ///
    /// # Errors
    ///
    /// [`SectorError::OffGrid`] unless `angle` is within `1e-9 Δ` of a grid
    /// angle.
    pub fn index_of_angle(&self, angle: f64) -> Result<i64, SectorError> {
        let step = self.step_angle();
        let k = (angle / step).round();
        if !angle.is_finite() || (angle - k * step).abs() > 1e-9 * step {
            return Err(SectorError::OffGrid { angle, step });
        }
        Ok(k as i64)
    }

    /// Expand a per-node vector on the input mesh to the split mesh (each
    /// split copy takes its original's value).
    ///
    /// # Panics
    ///
    /// Panics if `v.len()` is not the input-mesh node count.
    pub fn expand_nodal<T: Copy>(&self, v: &[T]) -> Vec<T> {
        assert_eq!(v.len(), self.n_original, "expand_nodal: input length");
        self.original.iter().map(|&o| v[o as usize]).collect()
    }

    /// The links at step `k`: the radial-edge pairs of both sides and the
    /// coupling re-connection `A_rotor[i] = s^w A_stator[(i+k) mod N]`.
    pub fn links(&self, k: i64) -> Vec<NodeLink> {
        let mut out = self.fixed_links.clone();
        let n = self.n as i64;
        for (i, &r) in self.rotor_coupling.iter().enumerate() {
            let j = i as i64 + k;
            let (w, m) = (j.div_euclid(n), j.rem_euclid(n));
            let sign = if self.full {
                1.0
            } else {
                self.periodicity.sign_pow(w)
            };
            out.push(NodeLink::new(
                r as usize,
                self.stator_coupling[m as usize] as usize,
                sign,
            ));
        }
        out
    }

    /// The radial-edge links only (both sides), without any coupling-circle
    /// re-connection: the links of [`CouplingMode::Harmonic`].
    pub fn edge_links(&self) -> Vec<NodeLink> {
        self.fixed_links.clone()
    }

    /// Check a set of harmonic coupling orders against this band and return
    /// it sorted and de-duplicated.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] for an empty set, an order without the
    /// sector symmetry (for a sector of angle `α`, `nα/π` must be an odd
    /// integer when antiperiodic and an even one when periodic), `n = 0`
    /// for an antiperiodic sector, or an order at or above the grid's
    /// Nyquist limit `nΔ ≥ π`.
    pub fn check_harmonic_orders(&self, orders: &[usize]) -> Result<Vec<usize>, SectorError> {
        let mut v = orders.to_vec();
        v.sort_unstable();
        v.dedup();
        if v.is_empty() {
            return Err(SectorError::InvalidInput(
                "harmonic coupling needs at least one order".into(),
            ));
        }
        let step = self.step_angle();
        for &n in &v {
            if !self.full {
                let q = n as f64 * self.sector_angle / std::f64::consts::PI;
                let qi = q.round();
                let parity_ok = match self.periodicity {
                    Periodicity::Antiperiodic => (qi as i64).rem_euclid(2) == 1,
                    Periodicity::Periodic => (qi as i64).rem_euclid(2) == 0,
                };
                if (q - qi).abs() > 1e-9 || !parity_ok {
                    return Err(SectorError::InvalidInput(format!(
                        "harmonic order {n} does not have the {:?} symmetry of a sector of \
                         {} rad",
                        self.periodicity, self.sector_angle
                    )));
                }
            }
            if n as f64 * step >= std::f64::consts::PI - 1e-12 {
                return Err(SectorError::InvalidInput(format!(
                    "harmonic order {n} is at or above the Nyquist limit of the coupling grid \
                     (n * step = {} >= pi, step {step} rad)",
                    n as f64 * step
                )));
            }
        }
        Ok(v)
    }

    /// Every order the band admits below its Nyquist limit (the full
    /// harmonic set, which spans the whole trace space).
    pub fn all_harmonic_orders(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut n = 0usize;
        while (n as f64) * self.step_angle() < std::f64::consts::PI - 1e-12 {
            if self.check_harmonic_orders(&[n]).is_ok() {
                out.push(n);
            }
            n += 1;
        }
        out
    }

    /// The radial-edge links without the pairs of the coupling-circle end
    /// nodes: the links of [`CouplingMode::HarmonicFreeEnds`], where the two
    /// end nodes of each side's coupling trace are tied only through the
    /// harmonic constraints.
    pub fn edge_links_free_ends(&self) -> Vec<NodeLink> {
        let ends: HashSet<usize> = [
            self.rotor_coupling[0],
            self.rotor_coupling[self.n],
            self.stator_coupling[0],
            self.stator_coupling[self.n],
        ]
        .iter()
        .map(|&x| x as usize)
        .collect();
        self.fixed_links
            .iter()
            .copied()
            .filter(|l| !(ends.contains(&l.node) && ends.contains(&l.master)))
            .collect()
    }

    /// Per-node moments `c_i = ∫ φ_i(θ) e^{inθ} r_c dθ` (as `[Re, Im]`, so
    /// the cosine moment is `Re` and the sine moment `Im`) of one side's
    /// coupling trace at step `k` (the rotor hats sit at `θ0 + (i + k)Δ`).
    ///
    /// With tied ends the last sector node `N` is the signed image of node
    /// `0` through the radial-edge links, and its half hat folds onto node
    /// `0`, so nodes `0..N` carry full hats `r_c Δ sinc²(nΔ/2) e^{inθ_i}`.
    /// With free ends nodes `0` and `N` carry their own half hats over the
    /// side's own arc.
    fn side_moments(&self, rotor: bool, k: i64, n: usize, free_ends: bool) -> Vec<(u32, [f64; 2])> {
        let step = self.step_angle();
        let nf = n as f64;
        let (nodes, k_eff) = if rotor {
            (&self.rotor_coupling, k)
        } else {
            (&self.stator_coupling, 0)
        };
        let angle = |i: usize| self.theta0 + (i as f64 + k_eff as f64) * step;
        let x = 0.5 * nf * step;
        let sinc = if x == 0.0 { 1.0 } else { x.sin() / x };
        let w = self.r_c * step * sinc * sinc;
        // Right half hat ∫_0^Δ (1 − t/Δ) e^{int} dt.
        let half = if n == 0 {
            [0.5 * step, 0.0]
        } else {
            let wd = nf * step;
            [
                (1.0 - wd.cos()) / (nf * nf * step),
                1.0 / nf - wd.sin() / (nf * nf * step),
            ]
        };
        let rot_c = |h: [f64; 2], th: f64| {
            let (c, s) = ((nf * th).cos(), (nf * th).sin());
            [
                self.r_c * (c * h[0] - s * h[1]),
                self.r_c * (s * h[0] + c * h[1]),
            ]
        };
        let mut out = Vec::with_capacity(self.n + 1);
        if free_ends && !self.full {
            out.push((nodes[0], rot_c(half, angle(0))));
            for (i, &node) in nodes.iter().enumerate().take(self.n).skip(1) {
                let th = angle(i);
                out.push((node, [w * (nf * th).cos(), w * (nf * th).sin()]));
            }
            out.push((nodes[self.n], rot_c([half[0], -half[1]], angle(self.n))));
        } else {
            for (i, &node) in nodes.iter().enumerate().take(self.n) {
                let th = angle(i);
                out.push((node, [w * (nf * th).cos(), w * (nf * th).sin()]));
            }
        }
        out
    }

    /// The harmonic coupling constraints at step `k` (see the module docs):
    /// for each order `n` the `sin nθ` and then the `cos nθ` moment (only
    /// `cos` for `n = 0`), stator minus rotor. `free_ends` selects the hat
    /// moments of [`CouplingMode::HarmonicFreeEnds`].
    ///
    /// # Errors
    ///
    /// As [`check_harmonic_orders`](Self::check_harmonic_orders), and
    /// [`SectorError::InvalidInput`] for `free_ends` on a full machine.
    pub fn harmonic_constraints(
        &self,
        k: i64,
        orders: &[usize],
        free_ends: bool,
    ) -> Result<LinearConstraints, SectorError> {
        let orders = self.check_harmonic_orders(orders)?;
        if free_ends && self.full {
            return Err(SectorError::InvalidInput(
                "a full machine has no coupling-circle ends to free".into(),
            ));
        }
        let mut cols = Vec::with_capacity(2 * orders.len());
        for &n in &orders {
            let st = self.side_moments(false, k, n, free_ends);
            let rt = self.side_moments(true, k, n, free_ends);
            // [Re, Im] index: sine is Im (1), cosine is Re (0).
            let parts: &[usize] = if n == 0 { &[0] } else { &[1, 0] };
            for &part in parts {
                let mut col = Vec::with_capacity(st.len() + rt.len());
                col.extend(st.iter().map(|&(node, c)| (node as usize, c[part])));
                col.extend(rt.iter().map(|&(node, c)| (node as usize, -c[part])));
                cols.push(col);
            }
        }
        LinearConstraints::new(self.mesh.n_nodes(), cols).map_err(SectorError::Solve)
    }

    /// The **coupling torque** of a harmonic-coupling solve at step `k`
    /// (N m per metre of axial length, on the modelled sector, positive
    /// counter-clockwise on the rotor): `T = −λᵀ ∂(Gᵀ A)/∂β`, where `∂/∂β`
    /// moves the rotor hat centres (module docs), `∂c_i/∂β = i n c_i`.
    /// `multipliers` are those of
    /// [`NonlinearMagnetostaticSystem::solve_constrained`] for
    /// [`harmonic_constraints`](Self::harmonic_constraints)`(k, orders, free_ends)`.
    ///
    /// # Errors
    ///
    /// As [`harmonic_constraints`](Self::harmonic_constraints), and
    /// [`SectorError::InvalidInput`] if `a_z` or `multipliers` have the
    /// wrong length.
    pub fn harmonic_coupling_torque(
        &self,
        k: i64,
        orders: &[usize],
        free_ends: bool,
        a_z: &[f64],
        multipliers: &[f64],
    ) -> Result<f64, SectorError> {
        let orders = self.check_harmonic_orders(orders)?;
        let m: usize = orders.iter().map(|&n| if n == 0 { 1 } else { 2 }).sum();
        if a_z.len() != self.mesh.n_nodes() || multipliers.len() != m {
            return Err(SectorError::InvalidInput(format!(
                "coupling torque: a_z length {} (need {}), {} multipliers (need {m})",
                a_z.len(),
                self.mesh.n_nodes(),
                multipliers.len()
            )));
        }
        let mut j = 0;
        let mut t = 0.0;
        for &n in &orders {
            let nf = n as f64;
            // The rotor entries are −c_i; d/dβ(−c_i) = −i n c_i, so
            // d(cos part)/dβ = n Im c_i and d(sin part)/dβ = −n Re c_i.
            let (mut dsin, mut dcos) = (0.0, 0.0);
            for (node, c) in self.side_moments(true, k, n, free_ends) {
                let a = a_z[node as usize];
                dsin += -nf * c[0] * a;
                dcos += nf * c[1] * a;
            }
            if n == 0 {
                t -= multipliers[j] * dcos;
                j += 1;
            } else {
                t -= multipliers[j] * dsin + multipliers[j + 1] * dcos;
                j += 2;
            }
        }
        Ok(t)
    }

    /// The rows of `∂(Gᵀ A)/∂β` behind
    /// [`harmonic_coupling_torque`](Self::harmonic_coupling_torque), one
    /// sparse row `(node, coefficient)` per constraint in the order of
    /// [`harmonic_constraints`](Self::harmonic_constraints), so that the
    /// coupling torque is `T = −Σ_j λ_j Σ_(i,w) w · A_i` (issue #1174: the
    /// shape adjoint needs `∂T/∂A` and `∂T/∂λ`). The entries sit on the
    /// rotor coupling nodes only.
    ///
    /// # Errors
    ///
    /// As [`harmonic_constraints`](Self::harmonic_constraints).
    pub fn harmonic_coupling_torque_rows(
        &self,
        k: i64,
        orders: &[usize],
        free_ends: bool,
    ) -> Result<Vec<Vec<(usize, f64)>>, SectorError> {
        let orders = self.check_harmonic_orders(orders)?;
        if free_ends && self.full {
            return Err(SectorError::InvalidInput(
                "a full machine has no coupling-circle ends to free".into(),
            ));
        }
        let mut rows = Vec::with_capacity(2 * orders.len());
        for &n in &orders {
            let nf = n as f64;
            let rt = self.side_moments(true, k, n, free_ends);
            // Same derivative as in `harmonic_coupling_torque`:
            // d(sin part)/dβ = −n Re c_i, d(cos part)/dβ = n Im c_i.
            if n != 0 {
                rows.push(
                    rt.iter()
                        .map(|&(node, c)| (node as usize, -nf * c[0]))
                        .collect(),
                );
            }
            rows.push(
                rt.iter()
                    .map(|&(node, c)| (node as usize, nf * c[1]))
                    .collect(),
            );
        }
        Ok(rows)
    }

    /// The coupling-circle nodes of the split mesh, rotor side then stator
    /// side, in grid order.
    pub fn coupling_nodes(&self) -> Vec<u32> {
        self.rotor_coupling
            .iter()
            .chain(&self.stator_coupling)
            .copied()
            .collect()
    }

    /// A copy of the band on the same mesh topology with the split-mesh
    /// node coordinates replaced by `nodes` (reference position `k = 0`):
    /// a moved design for the shape adjoint (issue #1174). The links, the
    /// coupling grid and the report are kept from `self`, so the move must
    /// leave the coupling-circle nodes **bit-for-bit** where they are, and
    /// it must not invert a triangle. Radial-edge nodes may move; the links
    /// stay the topological pairs of the reference mesh, and they still
    /// express the sector symmetry only if the move maps the low edge onto
    /// the high edge by the sector rotation.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] for a length mismatch, a non-finite
    /// coordinate, a moved coupling node, or a triangle whose signed area is
    /// not positive (the band's triangles are counter-clockwise).
    pub fn with_node_positions(&self, nodes: Vec<[f64; 2]>) -> Result<Self, SectorError> {
        let bad = |s: String| Err(SectorError::InvalidInput(s));
        if nodes.len() != self.mesh.n_nodes() {
            return bad(format!(
                "{} node positions for {} split-mesh nodes",
                nodes.len(),
                self.mesh.n_nodes()
            ));
        }
        if let Some(i) = nodes
            .iter()
            .position(|p| !(p[0].is_finite() && p[1].is_finite()))
        {
            return bad(format!("node {i} has a non-finite position"));
        }
        for &c in self.rotor_coupling.iter().chain(&self.stator_coupling) {
            let (old, new) = (self.mesh.nodes[c as usize], nodes[c as usize]);
            if old[0].to_bits() != new[0].to_bits() || old[1].to_bits() != new[1].to_bits() {
                return bad(format!(
                    "coupling-circle node {c} moved from ({:e}, {:e}) to ({:e}, {:e}); the \
                     coupling circle must stay fixed",
                    old[0], old[1], new[0], new[1]
                ));
            }
        }
        for (t, tri) in self.mesh.tris.iter().enumerate() {
            let [a, b, c] = tri.map(|i| nodes[i as usize]);
            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if cross.is_nan() || cross <= 0.0 {
                return bad(format!(
                    "triangle {t} is inverted or degenerate after the move (twice signed area \
                     {cross:e})"
                ));
            }
        }
        let mut out = self.clone();
        out.mesh.nodes = nodes;
        Ok(out)
    }

    /// The mesh at step `k`: rotor nodes rotated by `kΔ` about the origin,
    /// stator nodes unchanged.
    pub fn rotated_mesh(&self, k: i64) -> TriMesh {
        let th = self.angle_of(k);
        let (c, s) = (th.cos(), th.sin());
        let nodes = self
            .mesh
            .nodes
            .iter()
            .zip(&self.rotor_node)
            .map(|(&p, &r)| if r { rot(p, c, s) } else { p })
            .collect();
        TriMesh {
            nodes,
            tris: self.mesh.tris.clone(),
        }
    }

    /// Rotate a per-triangle vector field given at the reference position
    /// (a magnetization, say) to step `k`: rotor-triangle vectors turn by
    /// `kΔ`, stator ones are unchanged.
    ///
    /// # Panics
    ///
    /// Panics if `v.len()` is not the triangle count.
    pub fn rotate_rotor_vectors(&self, k: i64, v: &[[f64; 2]]) -> Vec<[f64; 2]> {
        assert_eq!(v.len(), self.mesh.n_tris(), "rotate_rotor_vectors: length");
        let th = self.angle_of(k);
        let (c, s) = (th.cos(), th.sin());
        v.iter()
            .zip(&self.rotor_tri)
            .map(|(&m, &r)| if r { rot(m, c, s) } else { m })
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Problem and solve
// ─────────────────────────────────────────────────────────────────────────

/// How the rotor and stator traces on the coupling circle are joined.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CouplingMode {
    /// Nodal re-connection `A_rotor[i] = s^w A_stator[i + k]` (the default):
    /// every harmonic the grid resolves is continuous.
    #[default]
    Conforming,
    /// Weak continuity of the sine and cosine moments of the listed angular
    /// orders only, through Lagrange multipliers (module docs, issue
    /// #1170). The ICEM reference uses `n = 3, 9, …, 45`. The end nodes of
    /// each side's coupling trace keep their radial-edge pairs.
    Harmonic(Vec<usize>),
    /// As [`Harmonic`](Self::Harmonic), but the end nodes of each side's
    /// coupling trace are left out of the radial-edge pairs and carry their
    /// own half hats ([`SlidingBand::edge_links_free_ends`]). This is how
    /// the ICEM reference treats them: its antiperiodic DOF sets exclude the
    /// coupling DOFs (`setAntiPeriodicBoundaries`). Sectors only.
    HarmonicFreeEnds(Vec<usize>),
}

impl CouplingMode {
    /// The harmonic orders and whether the ends are free (`None` for the
    /// conforming band).
    pub fn harmonic(&self) -> Option<(&[usize], bool)> {
        match self {
            Self::Conforming => None,
            Self::Harmonic(o) => Some((o, false)),
            Self::HarmonicFreeEnds(o) => Some((o, true)),
        }
    }
}

/// A nonlinear magnetostatic problem on a [`SlidingBand`]: materials,
/// sources in the reference position, and Dirichlet data, solvable at any
/// step `k`.
#[derive(Debug, Clone)]
pub struct SlidingBandProblem {
    band: SlidingBand,
    materials: MaterialMap,
    j_z: Vec<f64>,
    magnetization: Option<Vec<[f64; 2]>>,
    dirichlet: Vec<bool>,
    dirichlet_values: Option<Vec<f64>>,
    coupling: CouplingMode,
}

/// A converged solve at one sliding-band step.
#[derive(Debug, Clone)]
pub struct SlidingBandSolution {
    /// The step `k`.
    pub k: i64,
    /// The rotor angle `kΔ` (rad).
    pub angle: f64,
    /// The mesh at this step (rotor rotated, lab frame).
    pub mesh: TriMesh,
    /// Per-triangle flux density `B = (∂A/∂y, −∂A/∂x)` (T), lab frame.
    pub b: Vec<[f64; 2]>,
    /// The nonlinear solve: `A_z`, residual history, status, the reduced
    /// tangent `Pᵀ K P` and the prolongation data. Under
    /// [`CouplingMode::Harmonic`] the residual norms are those of the
    /// projected (KKT) residual and the tangent excludes the constraint
    /// rows (see [`NonlinearMagnetostaticSystem::solve_constrained`]).
    pub solution: NonlinearMagnetostaticSolution,
    /// The Lagrange multipliers of the harmonic coupling constraints, in the
    /// order of [`SlidingBand::harmonic_constraints`] (empty for
    /// [`CouplingMode::Conforming`]).
    pub multipliers: Vec<f64>,
    /// The coupling torque `−λᵀ ∂(Gᵀ A)/∂β` (N m per metre of axial length,
    /// modelled sector) for [`CouplingMode::Harmonic`]; `None` for the
    /// conforming band, where it is not defined.
    pub coupling_torque: Option<f64>,
    /// `max |Gᵀ A|` of the harmonic constraints at the solution (`0` for the
    /// conforming band).
    pub constraint_violation: f64,
    sector_angle: f64,
    periodicity: Periodicity,
}

impl SlidingBandSolution {
    /// Nodal `A_z` (Wb/m) on the split mesh.
    pub fn a_z(&self) -> &[f64] {
        &self.solution.a_z
    }

    /// How the solve converged.
    pub fn status(&self) -> crate::assembly::magnetostatic_nonlinear::ConvergenceStatus {
        self.solution.status
    }

    /// `B` (T) at a lab-frame point. Points outside the meshed region of
    /// this step (for example a rotor point that has rotated past the
    /// sector edge) are mapped through the sector symmetry,
    /// `B(R^m x) = s^m R^m B(x)`, for `|m| ≤ 2`. `None` if no image is in
    /// the mesh.
    pub fn sample_b(&self, x: f64, y: f64) -> Option<[f64; 2]> {
        let full = (self.sector_angle - TAU).abs() <= 1e-12;
        for m in [0i64, 1, -1, 2, -2] {
            if full && m != 0 {
                break;
            }
            let th = m as f64 * self.sector_angle;
            let q = rot([x, y], (-th).cos(), (-th).sin());
            if let Some(t) = locate_triangle(&self.mesh, q[0], q[1]) {
                let b = rot(self.b[t], th.cos(), th.sin());
                let s = self.periodicity.sign_pow(m);
                return Some([s * b[0], s * b[1]]);
            }
        }
        None
    }
}

impl SlidingBandProblem {
    /// Set up the problem.
    ///
    /// * `materials`: region tag → law (every triangle tag must be mapped).
    /// * `j_z`: per-triangle current density (A/m², SI), fixed to its
    ///   triangle (stator windings stay put, rotor bars turn).
    /// * `dirichlet`: per-node mask on the **input** mesh (split copies
    ///   inherit their original's flag).
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] on a length mismatch.
    pub fn new(
        band: SlidingBand,
        materials: MaterialMap,
        j_z: &[f64],
        dirichlet: &[bool],
    ) -> Result<Self, SectorError> {
        if j_z.len() != band.mesh.n_tris() {
            return Err(SectorError::InvalidInput(format!(
                "j_z length {} != triangle count {}",
                j_z.len(),
                band.mesh.n_tris()
            )));
        }
        if dirichlet.len() != band.n_original {
            return Err(SectorError::InvalidInput(format!(
                "dirichlet mask length {} != input node count {}",
                dirichlet.len(),
                band.n_original
            )));
        }
        let dirichlet = band.expand_nodal(dirichlet);
        Ok(Self {
            band,
            materials,
            j_z: j_z.to_vec(),
            magnetization: None,
            dirichlet,
            dirichlet_values: None,
            coupling: CouplingMode::Conforming,
        })
    }

    /// Choose how the two sides are joined on the coupling circle (default
    /// [`CouplingMode::Conforming`]). Harmonic orders are checked, sorted
    /// and de-duplicated here.
    ///
    /// # Errors
    ///
    /// As [`SlidingBand::check_harmonic_orders`].
    pub fn with_coupling(mut self, mode: CouplingMode) -> Result<Self, SectorError> {
        self.coupling = match mode {
            CouplingMode::Conforming => CouplingMode::Conforming,
            CouplingMode::Harmonic(orders) => {
                CouplingMode::Harmonic(self.band.check_harmonic_orders(&orders)?)
            }
            CouplingMode::HarmonicFreeEnds(orders) => {
                if self.band.full {
                    return Err(SectorError::InvalidInput(
                        "free coupling ends need a sector (a full machine has none)".into(),
                    ));
                }
                CouplingMode::HarmonicFreeEnds(self.band.check_harmonic_orders(&orders)?)
            }
        };
        Ok(self)
    }

    /// The coupling mode.
    pub fn coupling(&self) -> &CouplingMode {
        &self.coupling
    }

    /// Add a per-triangle magnetization (A/m) given at the reference
    /// position; rotor-triangle vectors are rotated with the rotor.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] on a length mismatch.
    pub fn with_magnetization(mut self, m: &[[f64; 2]]) -> Result<Self, SectorError> {
        if m.len() != self.band.mesh.n_tris() {
            return Err(SectorError::InvalidInput(format!(
                "magnetization length {} != triangle count {}",
                m.len(),
                self.band.mesh.n_tris()
            )));
        }
        self.magnetization = Some(m.to_vec());
        Ok(self)
    }

    /// Set Dirichlet values (Wb/m) per node of the **input** mesh.
    ///
    /// # Errors
    ///
    /// [`SectorError::InvalidInput`] on a length mismatch.
    pub fn with_dirichlet_values(mut self, values: &[f64]) -> Result<Self, SectorError> {
        if values.len() != self.band.n_original {
            return Err(SectorError::InvalidInput(format!(
                "dirichlet values length {} != input node count {}",
                values.len(),
                self.band.n_original
            )));
        }
        self.dirichlet_values = Some(self.band.expand_nodal(values));
        Ok(self)
    }

    /// The band.
    pub fn band(&self) -> &SlidingBand {
        &self.band
    }

    /// The constrained nonlinear system at step `k` and its mesh.
    ///
    /// # Errors
    ///
    /// [`SectorError::Solve`] from the system set-up (missing material,
    /// contradictory Dirichlet data under the links).
    pub fn system_at(
        &self,
        k: i64,
    ) -> Result<(TriMesh, NonlinearMagnetostaticSystem), SectorError> {
        let mesh = self.band.rotated_mesh(k);
        let mut sys = NonlinearMagnetostaticSystem::new(
            &mesh,
            &self.band.tri_tags,
            &self.materials,
            &self.j_z,
            &self.dirichlet,
        )?;
        if let Some(m) = &self.magnetization {
            sys = sys.with_magnetization(&self.band.rotate_rotor_vectors(k, m))?;
        }
        if let Some(v) = &self.dirichlet_values {
            sys = sys.with_dirichlet_values(v)?;
        }
        let links = match &self.coupling {
            CouplingMode::Conforming => self.band.links(k),
            CouplingMode::Harmonic(_) => self.band.edge_links(),
            CouplingMode::HarmonicFreeEnds(_) => self.band.edge_links_free_ends(),
        };
        sys = sys.with_node_links(&links)?;
        Ok((mesh, sys))
    }

    /// Solve at step `k` from `initial` (a full nodal vector on the split
    /// mesh, for example the previous step's `A_z`; it is projected onto
    /// the links of step `k`, and for [`CouplingMode::Harmonic`] onto the
    /// constraints), or from the linear start when `None` (for the harmonic
    /// coupling: from zero, whose first Newton step is the constrained
    /// linear solution when the Dirichlet data are zero).
    ///
    /// # Errors
    ///
    /// [`SectorError::Solve`] for any solver failure; non-convergence is
    /// never returned as a solution.
    pub fn solve_at(
        &self,
        k: i64,
        config: &NewtonConfig,
        initial: Option<&[f64]>,
    ) -> Result<SlidingBandSolution, SectorError> {
        let (mesh, sys) = self.system_at(k)?;
        let (solution, multipliers, coupling_torque, constraint_violation) = match &self.coupling {
            CouplingMode::Conforming => {
                let start;
                let init = match initial {
                    Some(a) => a,
                    None => {
                        start = sys.linear_start()?;
                        &start
                    }
                };
                (sys.solve(config, Some(init))?, Vec::new(), None, 0.0)
            }
            CouplingMode::Harmonic(orders) | CouplingMode::HarmonicFreeEnds(orders) => {
                let free = matches!(self.coupling, CouplingMode::HarmonicFreeEnds(_));
                let g = self.band.harmonic_constraints(k, orders, free)?;
                let cs = sys.solve_constrained(&g, config, initial)?;
                let t = self.band.harmonic_coupling_torque(
                    k,
                    orders,
                    free,
                    &cs.solution.a_z,
                    &cs.multipliers,
                )?;
                (
                    cs.solution,
                    cs.multipliers,
                    Some(t),
                    cs.constraint_violation,
                )
            }
        };
        let b = recover_b_field(&mesh, &solution.a_z);
        Ok(SlidingBandSolution {
            k,
            angle: self.band.angle_of(k),
            mesh,
            b,
            solution,
            multipliers,
            coupling_torque,
            constraint_violation,
            sector_angle: self.band.sector_angle,
            periodicity: self.band.periodicity,
        })
    }

    /// Solve at each step of `ks` in order, warm-starting every solve after
    /// the first from the previous step's `A_z`.
    ///
    /// # Errors
    ///
    /// The first [`solve_at`](Self::solve_at) error.
    pub fn sweep(
        &self,
        ks: &[i64],
        config: &NewtonConfig,
    ) -> Result<Vec<SlidingBandSolution>, SectorError> {
        let mut out: Vec<SlidingBandSolution> = Vec::with_capacity(ks.len());
        for &k in ks {
            let prev = out.last().map(|s| s.solution.a_z.clone());
            out.push(self.solve_at(k, config, prev.as_deref())?);
        }
        Ok(out)
    }
}
