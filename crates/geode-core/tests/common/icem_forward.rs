//! Shared model set-up for the ICEM reference motor's **initial** design
//! forward solve (motor paper M2, issue #1168): materials, magnet source,
//! phase currents, the conforming sliding band over the antiperiodic 60°
//! sector, and the torque estimators.
//!
//! Used by `examples/icem_initial_torque.rs` (the full L0–L3 run) and by
//! `tests/motor_icem_forward.rs` (the CI-fast L0 regression), with
//! `#[path = "common/icem_forward.rs"] mod icem;` (the example uses
//! `../tests/common/icem_forward.rs`).
//!
//! Every constant cites the `benchmarks/motor_reference/conventions.json`
//! field (or the `reference_data.json` / `geometry/README.md` entry) it
//! comes from. Nothing here is fitted to the reference torque.
#![allow(dead_code)] // each binary uses a subset

use std::f64::consts::PI;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use geode_core::analytic::slotless_pm::MU_0;
use geode_core::analytic::waveguide::TriMesh;
use geode_core::assembly::magnetostatic_nonlinear::{
    ConstantLaw, ConvergenceStatus, MaterialMap, NewtonConfig, NonlinearMagnetostaticError,
    ReluctivityLaw, TabulatedBhLaw,
};
use geode_core::assembly::magnetostatic_sector::{
    CouplingMode, MATCH_TOL_REL_DEFAULT, Periodicity, SectorBoundaryNodes, SectorError,
    SlidingBand, SlidingBandProblem, SlidingBandSolution, SlidingBandSpec,
};
use geode_core::assembly::torque::{arkkio_torque, cartesian_to_polar_b};
use geode_core::mesh::{TaggedTriMesh, read_tagged_tri_mesh};
use serde_json::Value;

// ── Tags (geometry/README.md, tag table) ────────────────────────────────
pub const TAG_ROTOR_IRON: i32 = 1;
pub const TAG_MAGNET: i32 = 2;
pub const TAG_ROTOR_AIR: i32 = 3;
pub const TAG_GAP_INNER: i32 = 4;
pub const TAG_GAP_OUTER: i32 = 5;
pub const TAG_STATOR_IRON: i32 = 6;
pub const TAG_SLOT_AIR: i32 = 7;
/// Winding group `g = 1..12` has tag `10 + g`.
pub const TAG_WINDING0: i32 = 10;
pub const LINE_DIRICHLET: [i32; 2] = [101, 102];
pub const LINE_EDGE_LO: [i32; 2] = [111, 113];
pub const LINE_EDGE_HI: [i32; 2] = [112, 114];
pub const LINE_COUPLING: [i32; 2] = [121, 122];

pub const ROTOR_TAGS: [i32; 4] = [TAG_ROTOR_IRON, TAG_MAGNET, TAG_ROTOR_AIR, TAG_GAP_INNER];

/// Stator-side triangle tags: outer gap band, stator iron, slot air and the
/// twelve winding groups.
pub fn stator_tags() -> Vec<i32> {
    let mut v = vec![TAG_GAP_OUTER, TAG_STATOR_IRON, TAG_SLOT_AIR];
    v.extend((1..=12).map(|g| TAG_WINDING0 + g));
    v
}

// ── Geometry / machine constants (conventions.json) ─────────────────────
/// `geometry.modeled_sector_degrees` = 60.
pub const SECTOR: f64 = PI / 3.0;
/// `geometry.pole_pairs` = 3.
pub const POLE_PAIRS: f64 = 3.0;
/// `geometry.initial_axial_length_m` = 0.1 (torque includes it; SymMult = 1,
/// `torque.reported_units`).
pub const L_AXIAL: f64 = 0.1;
/// `geometry.initial_rotor_outer_radius_m` = 0.044 (inner edge of the
/// Arkkio band, tag 4).
pub const R_ROTOR: f64 = 0.044;
/// `geometry.initial_circular_coupling_radius_m` = 0.0445.
pub const R_COUPLING: f64 = 0.0445;
/// `geometry.initial_stator_inner_radius_m` = 0.045 (outer edge of tag 5).
pub const R_STATOR: f64 = 0.045;

// ── Materials (conventions.json `materials`) ────────────────────────────
/// `materials.magnet_Br_T`.
pub const MAGNET_BR: f64 = 1.4;
/// `materials.magnet_relative_recoil_permeability`.
pub const MAGNET_MUR: f64 = 1.05;
/// `materials.magnet_orientation_in_rotor_frame_radians` (π/2, +y).
pub const MAGNET_ANGLE: f64 = PI / 2.0;

/// Coercive field of the reference source, `Hc = Br / (μ0 μr)` (A/m)
/// (`materials.magnet_source`: `addRemanenceRotor` multiplies
/// `op_gradv_n_bot` by `Br / getMuLinear()`, `getMuLinear = Mur·Mu0`).
///
/// **Mapping.** The reference load is `Hc ∫ ∇v · (−sin α, cos α)`. GEODE's
/// magnetization load (`with_magnetization`) is
/// `∫ M · (∇φ)^⊥ = ∫ (M_x ∂_yφ − M_y ∂_xφ)`, which for
/// `M = Hc (cos α, sin α)` is the same integrand. GEODE's constitutive law
/// is `H = ν B − M` with `ν = 1/(μ0 μr)`, so `M = Hc` gives
/// `B = μ0 μr H + Br`: remanence exactly `Br`. Passing `Br/μ0` (the
/// `radial_magnetization_source_from_remanence` scaling) instead would give
/// remanence `μr Br = 1.47 T`, 5 % too strong.
pub fn magnet_hc() -> f64 {
    MAGNET_BR / (MU_0 * MAGNET_MUR)
}

/// The reference's coupling harmonic orders, `n = 3:6:45`
/// (`boundaries.coupling_sine_and_cosine_harmonics`): 8 orders, 16
/// sine/cosine coupling modes.
pub const REFERENCE_HARMONICS: [usize; 8] = [3, 9, 15, 21, 27, 33, 39, 45];

// ── Excitation (conventions.json `excitation`) ──────────────────────────
/// `excitation.winding_turns`.
pub const TURNS: f64 = 12.0;
/// `excitation.initial_reference_peak_current_A`.
pub const I_PEAK_INITIAL: f64 = 9.677_657_374_172_274;
/// `excitation.levels`.
pub const LEVELS: [f64; 3] = [0.0, 0.5, 1.0];
/// `excitation.initial_load_angle_degrees` (φ0).
pub const PHI0_INITIAL_DEG: f64 = 0.0;

/// Phase of a winding group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    U,
    V,
    W,
}

impl Phase {
    /// `excitation.phase_offsets_radians`: U = 0, V = −2π/3, W = +2π/3.
    pub fn offset(self) -> f64 {
        match self {
            Phase::U => 0.0,
            Phase::V => -2.0 * PI / 3.0,
            Phase::W => 2.0 * PI / 3.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Phase::U => "U",
            Phase::V => "V",
            Phase::W => "W",
        }
    }
}

/// Winding groups 1..12 in `reference_data.json`
/// `winding_groups_one_based_stator_patch_ids` order (the tag table's
/// `winding_gg_P+-` names): `(phase, sign)`.
pub const WINDING_GROUPS: [(Phase, f64); 12] = [
    (Phase::U, 1.0),
    (Phase::U, 1.0),
    (Phase::U, 1.0),
    (Phase::U, -1.0),
    (Phase::W, -1.0),
    (Phase::W, -1.0),
    (Phase::W, -1.0),
    (Phase::W, -1.0),
    (Phase::V, 1.0),
    (Phase::V, 1.0),
    (Phase::V, 1.0),
    (Phase::V, 1.0),
];

/// Instantaneous phase current `I_q = I_peak sin(φ0 + 3β + offset_q)`
/// (`excitation.current_convention`; β mechanical, rad).
pub fn phase_current(i_peak: f64, phi0: f64, beta: f64, phase: Phase) -> f64 {
    i_peak * (phi0 + POLE_PAIRS * beta + phase.offset()).sin()
}

// ── Low-field law variant (sensitivity) ─────────────────────────────────

/// Which M27 law the iron takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IronLaw {
    /// GEODE's `TabulatedBhLaw::from_hb` on the 47 M27 points (the #1160
    /// law: odd-cubic first segment, `ν(0) = 397.66 m/H`).
    Table,
    /// The reference's first segment: MATLAB `pchip` on `[0, 0.05]` T with
    /// its one-sided endpoint slope (426.04 m/H) and the `B ≤ 1e-5` T clamp
    /// of `Mat_Magnetic.getMu` (`ν = 426.03 m/H` there). Identical to
    /// [`IronLaw::Table`] above 0.05 T.
    RefClamp426,
}

impl IronLaw {
    pub fn name(self) -> &'static str {
        match self {
            IronLaw::Table => "table",
            IronLaw::RefClamp426 => "clamp426",
        }
    }
}

/// [`IronLaw::RefClamp426`]: the reference's cubic Hermite first segment
/// `H(B)`, `B ∈ [0, B1]`, with end slopes `m0` (MATLAB pchip one-sided
/// three-point formula) and `m1` (the shared Fritsch–Butland interior slope,
/// identical in both implementations per `conventions.json`
/// `geode_law_vs_reference.interior_0p05_to_2p25_T`), and `ν` held at
/// `ν(B_CLAMP)` below `B_CLAMP = 1e-5` T. Above `B1` it is the base table.
/// `ν`, `dν/d(b²)` and the energy are mutually consistent (the energy is
/// the exact integral of the `H` used), so Newton and the energy line search
/// see one law.
#[derive(Debug, Clone)]
pub struct ClampedLowFieldLaw {
    base: TabulatedBhLaw,
    b1: f64,
    h1: f64,
    m0: f64,
    m1: f64,
    nu_clamp: f64,
    w_clamp: f64,
    w_b1_self: f64,
    w_b1_base: f64,
}

/// `Mat_Magnetic.getMu` clamp (`materials.origin_and_tail_caveat`).
pub const B_CLAMP: f64 = 1e-5;

impl ClampedLowFieldLaw {
    pub fn new(base: TabulatedBhLaw) -> Self {
        let b = base.b_points().to_vec();
        let h = base.h_points().to_vec();
        let (b1, h1) = (b[1], h[1]);
        let d1 = (h[1] - h[0]) / (b[1] - b[0]);
        let d2 = (h[2] - h[1]) / (b[2] - b[1]);
        let (hh1, hh2) = (b[1] - b[0], b[2] - b[1]);
        // MATLAB pchip endpoint slope (one-sided three-point, shape clamps).
        let mut m0 = ((2.0 * hh1 + hh2) * d1 - hh1 * d2) / (hh1 + hh2);
        if m0.signum() != d1.signum() {
            m0 = 0.0;
        } else if d1.signum() != d2.signum() && m0.abs() > (3.0 * d1).abs() {
            m0 = 3.0 * d1;
        }
        let m1 = base.dh_db(b1);
        let mut law = Self {
            base,
            b1,
            h1,
            m0,
            m1,
            nu_clamp: 0.0,
            w_clamp: 0.0,
            w_b1_self: 0.0,
            w_b1_base: 0.0,
        };
        law.nu_clamp = law.seg_h(B_CLAMP).0 / B_CLAMP;
        law.w_clamp = 0.5 * law.nu_clamp * B_CLAMP * B_CLAMP;
        law.w_b1_self = law.w_clamp + law.seg_integral(B_CLAMP, b1);
        law.w_b1_base = law.base.energy_density(b1 * b1);
        law
    }

    /// Endpoint slope at `B = 0` (m/H).
    pub fn m0(&self) -> f64 {
        self.m0
    }

    /// `ν` on the clamp `B ≤ 1e-5` T (m/H).
    pub fn nu_clamp(&self) -> f64 {
        self.nu_clamp
    }

    /// `(H, dH/dB)` on the first-segment cubic.
    fn seg_h(&self, bb: f64) -> (f64, f64) {
        let dx = self.b1;
        let t = bb / dx;
        let (t2, t3) = (t * t, t * t * t);
        let val = (t3 - 2.0 * t2 + t) * dx * self.m0
            + (-2.0 * t3 + 3.0 * t2) * self.h1
            + (t3 - t2) * dx * self.m1;
        let slope = (3.0 * t2 - 4.0 * t + 1.0) * self.m0
            + (-6.0 * t2 + 6.0 * t) * self.h1 / dx
            + (3.0 * t2 - 2.0 * t) * self.m1;
        (val, slope)
    }

    /// `∫_lo^hi H dB` on the cubic (3-point Gauss–Legendre, exact).
    fn seg_integral(&self, lo: f64, hi: f64) -> f64 {
        let half = 0.5 * (hi - lo);
        let mid = 0.5 * (hi + lo);
        let x = (0.6_f64).sqrt();
        half * (5.0 / 9.0 * self.seg_h(mid - half * x).0
            + 8.0 / 9.0 * self.seg_h(mid).0
            + 5.0 / 9.0 * self.seg_h(mid + half * x).0)
    }
}

impl ReluctivityLaw for ClampedLowFieldLaw {
    fn nu(&self, b2: f64) -> f64 {
        let bb = b2.max(0.0).sqrt();
        if bb <= B_CLAMP {
            self.nu_clamp
        } else if bb < self.b1 {
            self.seg_h(bb).0 / bb
        } else {
            self.base.nu(b2)
        }
    }

    fn dnu_db2(&self, b2: f64) -> f64 {
        let bb = b2.max(0.0).sqrt();
        if bb <= B_CLAMP {
            0.0
        } else if bb < self.b1 {
            let (hv, hp) = self.seg_h(bb);
            (hp * bb - hv) / (2.0 * bb * bb * bb)
        } else {
            self.base.dnu_db2(b2)
        }
    }

    fn energy_density(&self, b2: f64) -> f64 {
        let bb = b2.max(0.0).sqrt();
        if bb <= B_CLAMP {
            0.5 * self.nu_clamp * b2
        } else if bb < self.b1 {
            self.w_clamp + self.seg_integral(B_CLAMP, bb)
        } else {
            self.base.energy_density(b2) - self.w_b1_base + self.w_b1_self
        }
    }
}

/// The M27 table from `reference_data.json` `material`.
pub fn m27_table(reference: &Value) -> (Vec<f64>, Vec<f64>) {
    let get = |k: &str| -> Vec<f64> {
        reference["material"][k]
            .as_array()
            .expect("M27 array")
            .iter()
            .map(|v| v.as_f64().expect("number"))
            .collect()
    };
    (get("H_A_per_m"), get("B_T"))
}

/// Material map: M27 in rotor iron (tag 1, which includes the bridges) and
/// stator iron (tag 6); the magnet linear with μr = 1.05; air, gap, slot
/// air and copper μr = 1 (`materials.air_and_copper_relative_permeability`).
pub fn materials(
    reference: &Value,
    law: IronLaw,
) -> Result<MaterialMap, NonlinearMagnetostaticError> {
    let (h, b) = m27_table(reference);
    let table = TabulatedBhLaw::from_hb(&h, &b)?;
    let iron: Arc<dyn ReluctivityLaw> = match law {
        IronLaw::Table => Arc::new(table),
        IronLaw::RefClamp426 => Arc::new(ClampedLowFieldLaw::new(table)),
    };
    let mut m = MaterialMap::new();
    m.insert_shared(TAG_ROTOR_IRON, iron.clone());
    m.insert_shared(TAG_STATOR_IRON, iron);
    m.insert(
        TAG_MAGNET,
        ConstantLaw::from_relative_permeability(MAGNET_MUR)?,
    );
    for t in [TAG_ROTOR_AIR, TAG_GAP_INNER, TAG_GAP_OUTER, TAG_SLOT_AIR] {
        m.insert(t, ConstantLaw::vacuum());
    }
    for g in 1..=12 {
        m.insert(TAG_WINDING0 + g, ConstantLaw::vacuum());
    }
    Ok(m)
}

// ── Paths ────────────────────────────────────────────────────────────────
pub fn motor_reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/motor_reference")
}

pub fn mesh_path(level: usize) -> PathBuf {
    motor_reference_dir()
        .join("geometry/meshes")
        .join(format!("icem_initial_sector_L{level}.msh"))
}

pub fn load_reference() -> Value {
    let p = motor_reference_dir().join("reference_data.json");
    serde_json::from_slice(&std::fs::read(&p).expect("read reference_data.json"))
        .expect("parse reference_data.json")
}

pub fn load_exact_areas() -> Value {
    let p = motor_reference_dir().join("geometry/exact_areas.json");
    serde_json::from_slice(&std::fs::read(&p).expect("read exact_areas.json"))
        .expect("parse exact_areas.json")
}

/// Exact NURBS area (m²) of winding group `g` (1-based) from
/// `geometry/exact_areas.json`.
pub fn exact_group_area(exact: &Value, g: i32) -> f64 {
    let groups = exact["winding_groups"].as_array().expect("winding_groups");
    let e = &groups[(g - 1) as usize];
    e["area_m2"].as_f64().expect("area_m2")
}

// ── Model ────────────────────────────────────────────────────────────────

/// Which winding-group area the current density uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaMode {
    /// The group's own mesh area (the issue's primary rule).
    Mesh,
    /// The exact NURBS area (cross-check).
    Exact,
}

/// The ICEM initial design on one mesh level.
pub struct IcemModel {
    pub level: usize,
    pub tagged: TaggedTriMesh,
    pub band: SlidingBand,
    pub materials: MaterialMap,
    /// Per-input-node Dirichlet mask (lines 101/102).
    pub dirichlet: Vec<bool>,
    /// Group areas (m²) used for `J`, index `g − 1`.
    pub group_areas: [f64; 12],
    /// Mesh areas of the groups (m²).
    pub mesh_group_areas: [f64; 12],
    /// Per-triangle magnetization at the reference position (A/m).
    pub magnetization: Vec<[f64; 2]>,
    /// Triangles of the two gap bands (tags 4, 5) for the Maxwell sampler.
    pub gap_tris: Vec<usize>,
    /// Rotor-stator coupling on r_c (conforming band by default; #1170 adds
    /// the reference's harmonic coupling).
    pub coupling: CouplingMode,
}

impl IcemModel {
    /// Build the model from a mesh file.
    pub fn new(
        level: usize,
        msh: &[u8],
        reference: &Value,
        law: IronLaw,
        area_mode: AreaMode,
        exact: Option<&Value>,
    ) -> Result<Self, SectorError> {
        let tagged = read_tagged_tri_mesh(msh)
            .map_err(|e| SectorError::InvalidInput(format!("read mesh: {e}")))?;
        let nodes = SectorBoundaryNodes::from_tagged_edges(
            &tagged.lines,
            &tagged.line_physical_tags,
            &LINE_COUPLING,
            &LINE_EDGE_LO,
            &LINE_EDGE_HI,
        )?;
        let spec = SlidingBandSpec {
            sector_angle: SECTOR,
            periodicity: Periodicity::Antiperiodic,
            rotor_tags: ROTOR_TAGS.to_vec(),
            stator_tags: stator_tags(),
            match_tol_rel: MATCH_TOL_REL_DEFAULT,
        };
        let band = SlidingBand::new(&tagged.mesh, &tagged.tri_physical_tags, &nodes, &spec)?;
        let mut dirichlet = vec![false; tagged.mesh.n_nodes()];
        for t in LINE_DIRICHLET {
            for n in tagged.nodes_on_line_tag(t) {
                dirichlet[n as usize] = true;
            }
        }
        let mut mesh_group_areas = [0.0; 12];
        let mut group_areas = [0.0; 12];
        for g in 1..=12 {
            mesh_group_areas[(g - 1) as usize] = tagged.area_with_tag(TAG_WINDING0 + g);
            group_areas[(g - 1) as usize] = match area_mode {
                AreaMode::Mesh => mesh_group_areas[(g - 1) as usize],
                AreaMode::Exact => exact_group_area(exact.expect("exact areas"), g),
            };
        }
        let hc = magnet_hc();
        let mvec = [hc * MAGNET_ANGLE.cos(), hc * MAGNET_ANGLE.sin()];
        let magnetization = band
            .tri_tags()
            .iter()
            .map(|&t| if t == TAG_MAGNET { mvec } else { [0.0, 0.0] })
            .collect();
        let gap_tris = band
            .tri_tags()
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == TAG_GAP_INNER || **t == TAG_GAP_OUTER)
            .map(|(i, _)| i)
            .collect();
        let materials = materials(reference, law)?;
        Ok(Self {
            level,
            tagged,
            band,
            materials,
            dirichlet,
            group_areas,
            mesh_group_areas,
            magnetization,
            gap_tris,
            coupling: CouplingMode::Conforming,
        })
    }

    /// Steps per mechanical degree.
    pub fn steps_per_degree(&self) -> i64 {
        self.band
            .index_of_angle(PI / 180.0)
            .expect("1 degree is on the coupling grid")
    }

    /// Per-triangle `J_z` (A/m²) at mechanical angle `beta` (rad):
    /// group `g` carries `I_q · 12 · sign / area_g`
    /// (`excitation.current_density_rule`).
    pub fn current_density(&self, i_peak: f64, phi0: f64, beta: f64) -> Vec<f64> {
        let mut jg = [0.0; 12];
        for (g, &(ph, sign)) in WINDING_GROUPS.iter().enumerate() {
            jg[g] = phase_current(i_peak, phi0, beta, ph) * TURNS * sign / self.group_areas[g];
        }
        self.band
            .tri_tags()
            .iter()
            .map(|&t| {
                let g = t - TAG_WINDING0;
                if (1..=12).contains(&g) {
                    jg[(g - 1) as usize]
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// The problem at mechanical angle `beta` (the currents depend on it).
    pub fn problem(
        &self,
        i_peak: f64,
        phi0: f64,
        beta: f64,
    ) -> Result<SlidingBandProblem, SectorError> {
        let j = self.current_density(i_peak, phi0, beta);
        SlidingBandProblem::new(
            self.band.clone(),
            self.materials.clone(),
            &j,
            &self.dirichlet,
        )?
        .with_magnetization(&self.magnetization)?
        .with_coupling(self.coupling.clone())
    }

    /// Solve at integer mechanical degree `deg`, warm-started from `prev`.
    pub fn solve_degree(
        &self,
        i_peak: f64,
        phi0: f64,
        deg: i64,
        cfg: &NewtonConfig,
        prev: Option<&[f64]>,
    ) -> Result<(SlidingBandSolution, f64), SectorError> {
        let beta = deg as f64 * PI / 180.0;
        let k = deg * self.steps_per_degree();
        let t0 = Instant::now();
        let p = self.problem(i_peak, phi0, beta)?;
        let s = p.solve_at(k, cfg, prev)?;
        Ok((s, t0.elapsed().as_secs_f64()))
    }

    /// Arkkio torque (N m, per modelled sector, axial length included) over
    /// the gap bands selected by `bands`: `0` inner (44.0–44.5 mm), `1`
    /// outer (44.5–45.0 mm), `2` both (44.0–45.0 mm).
    pub fn arkkio(&self, s: &SlidingBandSolution, bands: usize) -> f64 {
        let tags = self.band.tri_tags();
        match bands {
            0 => arkkio_torque(
                &s.mesh,
                tags,
                &s.b,
                TAG_GAP_INNER,
                R_ROTOR,
                R_COUPLING,
                L_AXIAL,
            ),
            1 => arkkio_torque(
                &s.mesh,
                tags,
                &s.b,
                TAG_GAP_OUTER,
                R_COUPLING,
                R_STATOR,
                L_AXIAL,
            ),
            _ => {
                arkkio_torque(
                    &s.mesh,
                    tags,
                    &s.b,
                    TAG_GAP_INNER,
                    R_ROTOR,
                    R_STATOR,
                    L_AXIAL,
                ) + arkkio_torque(
                    &s.mesh,
                    tags,
                    &s.b,
                    TAG_GAP_OUTER,
                    R_ROTOR,
                    R_STATOR,
                    L_AXIAL,
                )
            }
        }
    }

    /// Maxwell-stress torque on the circle of radius `r` over the sector,
    /// `T = (L r²/μ0) ∫_{60°}^{120°} B_r B_θ dθ` (midpoint rule, `n` points;
    /// `B_r B_θ` is invariant under the antiperiodic image, so the sector
    /// integral is the sector torque). Points are located among the gap-band
    /// triangles only, through the sector images for rotated rotor
    /// triangles.
    pub fn maxwell(&self, s: &SlidingBandSolution, r: f64, n: usize) -> f64 {
        let loc = GapLocator::new(&s.mesh, &self.gap_tris);
        let (br, bt) = ring_samples(&loc, &s.b, r, n);
        let dth = SECTOR / n as f64;
        let acc: f64 = br.iter().zip(&bt).map(|(a, b)| a * b).sum();
        L_AXIAL * r * r / MU_0 * acc * dth
    }

    /// **Harmonic-resolved air-gap torque** (diagnostic for the reference's
    /// truncated harmonic coupling, `boundaries.coupling_sine_and_cosine_harmonics`
    /// = 3, 9, …, 45). On `n_rings` circles at the midpoints of equal
    /// sub-intervals of the 44.0–45.0 mm band, `B_r` and `B_θ` are sampled
    /// at `n_theta` midpoints over the sector and expanded in the
    /// antiperiodic harmonics `n = 3, 9, 15, …`
    /// (`a_n = (6/π) ∫_sector f cos nθ dθ`). The ring torque restricted to
    /// harmonics `n ≤ cut` is `(L r²/μ0)(π/6) Σ_{n≤cut} (a_n^r a_n^θ + b_n^r b_n^θ)`
    /// (Parseval over the sector); the ring average is the Arkkio estimator
    /// by midpoint quadrature in `r`. Returns `(full, [partial per cut])`,
    /// `full` being the ring average of the sampled `∫ B_r B_θ dθ`.
    pub fn harmonic_torque(
        &self,
        s: &SlidingBandSolution,
        n_rings: usize,
        n_theta: usize,
        cuts: &[usize],
    ) -> (f64, Vec<f64>) {
        let loc = GapLocator::new(&s.mesh, &self.gap_tris);
        let dth = SECTOR / n_theta as f64;
        let max_cut = cuts.iter().copied().max().unwrap_or(0);
        let mut full = 0.0;
        let mut part = vec![0.0; cuts.len()];
        for j in 0..n_rings {
            let r = R_ROTOR + (j as f64 + 0.5) * (R_STATOR - R_ROTOR) / n_rings as f64;
            let (br, bt) = ring_samples(&loc, &s.b, r, n_theta);
            let pref = L_AXIAL * r * r / MU_0;
            full += pref * dth * br.iter().zip(&bt).map(|(a, b)| a * b).sum::<f64>();
            let mut n = 3;
            while n <= max_cut {
                let (mut arc, mut ars, mut atc, mut ats) = (0.0, 0.0, 0.0, 0.0);
                for i in 0..n_theta {
                    let th = SECTOR + (i as f64 + 0.5) * dth;
                    let (c, sn) = ((n as f64 * th).cos(), (n as f64 * th).sin());
                    arc += br[i] * c;
                    ars += br[i] * sn;
                    atc += bt[i] * c;
                    ats += bt[i] * sn;
                }
                let k = 6.0 / PI * dth;
                let t_n = pref * PI / 6.0 * (k * arc * k * atc + k * ars * k * ats);
                for (ci, &cut) in cuts.iter().enumerate() {
                    if n <= cut {
                        part[ci] += t_n;
                    }
                }
                n += 6;
            }
        }
        let m = n_rings as f64;
        (full / m, part.into_iter().map(|p| p / m).collect())
    }

    /// Maximum |B| over iron triangles (T).
    pub fn iron_b_max(&self, s: &SlidingBandSolution) -> f64 {
        self.band
            .tri_tags()
            .iter()
            .zip(&s.b)
            .filter(|(t, _)| **t == TAG_ROTOR_IRON || **t == TAG_STATOR_IRON)
            .map(|(_, b)| b[0].hypot(b[1]))
            .fold(0.0, f64::max)
    }
}

/// Point location among the gap-band triangles of one solved (rotated)
/// mesh, bucketed by polar angle. Rotor triangles that have rotated past
/// the sector edge are reached through the antiperiodic images.
pub struct GapLocator {
    mesh: TriMesh,
    map: Vec<usize>,
    theta_min: f64,
    width: f64,
    buckets: Vec<Vec<u32>>,
}

impl GapLocator {
    pub fn new(mesh: &TriMesh, gap_tris: &[usize]) -> Self {
        let sub = TriMesh {
            nodes: mesh.nodes.clone(),
            tris: gap_tris.iter().map(|&t| mesh.tris[t]).collect(),
        };
        let ang = |p: [f64; 2]| p[1].atan2(p[0]);
        let spans: Vec<(f64, f64)> = sub
            .tris
            .iter()
            .map(|t| {
                let a = t.map(|i| ang(sub.nodes[i as usize]));
                (
                    a.iter().copied().fold(f64::INFINITY, f64::min),
                    a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                )
            })
            .collect();
        let theta_min = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min) - 1e-9;
        let theta_max = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max) + 1e-9;
        let nb = (sub.tris.len() / 4).max(16);
        let width = (theta_max - theta_min) / nb as f64;
        let mut buckets = vec![Vec::new(); nb];
        for (t, &(lo, hi)) in spans.iter().enumerate() {
            let a = (((lo - theta_min) / width).floor() as isize - 1).max(0) as usize;
            let b =
                ((((hi - theta_min) / width).floor() as isize) + 1).min(nb as isize - 1) as usize;
            for bk in buckets.iter_mut().take(b + 1).skip(a) {
                bk.push(t as u32);
            }
        }
        Self {
            mesh: sub,
            map: gap_tris.to_vec(),
            theta_min,
            width,
            buckets,
        }
    }

    fn locate(&self, x: f64, y: f64) -> Option<usize> {
        let th = y.atan2(x);
        let k = ((th - self.theta_min) / self.width).floor();
        if k < 0.0 || k as usize >= self.buckets.len() {
            return None;
        }
        for &t in &self.buckets[k as usize] {
            let [p1, p2, p3] = self.mesh.tris[t as usize].map(|i| self.mesh.nodes[i as usize]);
            // Barycentric test with the tolerance of `locate_triangle`.
            let d = (p2[1] - p3[1]) * (p1[0] - p3[0]) + (p3[0] - p2[0]) * (p1[1] - p3[1]);
            let a = ((p2[1] - p3[1]) * (x - p3[0]) + (p3[0] - p2[0]) * (y - p3[1])) / d;
            let b = ((p3[1] - p1[1]) * (x - p3[0]) + (p1[0] - p3[0]) * (y - p3[1])) / d;
            let tol = -1e-9;
            if a >= tol && b >= tol && 1.0 - a - b >= tol {
                return Some(self.map[t as usize]);
            }
        }
        None
    }

    /// `B` at `(x, y)`, trying the antiperiodic images `R^m`, `m ∈ {0, ±1}`:
    /// `B(R^m q) = (−1)^m R^m B(q)`.
    pub fn sample(&self, b: &[[f64; 2]], x: f64, y: f64) -> Option<[f64; 2]> {
        for m in [0i32, 1, -1] {
            let th = m as f64 * SECTOR;
            let (c, s) = (th.cos(), th.sin());
            let q = [c * x + s * y, -s * x + c * y]; // q = R^{-m} p
            if let Some(t) = self.locate(q[0], q[1]) {
                let bb = b[t];
                let rb = [c * bb[0] - s * bb[1], s * bb[0] + c * bb[1]];
                let sign = if m % 2 == 0 { 1.0 } else { -1.0 };
                return Some([sign * rb[0], sign * rb[1]]);
            }
        }
        None
    }
}

/// `(B_r, B_θ)` at `n` midpoints over the lab-frame sector `[60°, 120°]` on
/// the circle of radius `r`.
fn ring_samples(loc: &GapLocator, b: &[[f64; 2]], r: f64, n: usize) -> (Vec<f64>, Vec<f64>) {
    let dth = SECTOR / n as f64;
    let mut br = Vec::with_capacity(n);
    let mut bt = Vec::with_capacity(n);
    for i in 0..n {
        let th = SECTOR + (i as f64 + 0.5) * dth;
        let v = loc
            .sample(b, r * th.cos(), r * th.sin())
            .unwrap_or_else(|| panic!("circle r = {r} theta = {th}: outside the gap bands"));
        let (a, t) = cartesian_to_polar_b(v, th);
        br.push(a);
        bt.push(t);
    }
    (br, bt)
}

/// Status label for a converged solve.
pub fn status_name(s: ConvergenceStatus) -> &'static str {
    match s {
        ConvergenceStatus::Tolerance => "tolerance",
        ConvergenceStatus::RoundoffFloor => "roundoff_floor",
    }
}

/// Default (`rel_tol = 1e-8`) and tight (`rel_tol = 1e-12`) Newton settings.
pub fn newton_config(tight: bool) -> NewtonConfig {
    NewtonConfig {
        rel_tol: if tight { 1e-12 } else { 1e-8 },
        max_iterations: 60,
        ..NewtonConfig::default()
    }
}
