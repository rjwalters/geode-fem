//! Effectivity and stopping study of the goal-oriented peak-field adaptive
//! loop (issue #1767, parent #1481).
//!
//! Two **distinct geometries with nonzero reference errors** are driven
//! through [`adapt_peak_field`] and printed as reproducible effectivity
//! tables:
//!
//! ```sh
//! # geometry A (default tier):
//! cargo test -p geode-core --test adaptive_peak_field -- --nocapture
//! # both tables (geometry B's reference solves are release-tier only,
//! # #[cfg_attr(debug_assertions, ignore)] like cube_convergence_regression):
//! cargo test -p geode-core --release --test adaptive_peak_field -- --include-ignored -- --nocapture
//! ```
//!
//! 1. **Nonuniformly loaded plates** (curved field, analytic solution):
//!    `φ = sin(πz)` with `ρ = ε₀π²sin(πz)`, both plates `z = 0/1`
//!    grounded, natural side walls — `−∇·(ε₀∇φ) = ρ` and all boundary
//!    data are consistent by construction. The reference is the
//!    observable's **analytic limit** — the domain-clipped window average
//!    of `|E| = πcos(πz)`, `sin(π·w/2)/(w/2)` for window edge `w` — which
//!    bounds every mesh's window average from above because the field is
//!    monotone toward the plate; the residual mesh-dependence of the
//!    discrete family is quantified by analytic-sampled scans (n = 24 vs
//!    n = 32). Same physical region (3-D tet tags, carried by the
//!    refinement) and window edge on every mesh.
//! 2. **Edge problem** (release tier): ground on `z = 0`, a half plate at
//!    1 V on `z = 1, x ≤ 0.5`, natural elsewhere. The Dirichlet–Neumann
//!    edge `x = 0.5, z = 1` produces the classic `|E| ∝ r^(−1/2)` plate
//!    edge enhancement with consistent PDE and boundary data. The
//!    reference is a **finer solve** of the same discrete problem on
//!    uniform meshes (n = 32, sensitivity n = 40), same region and window
//!    edge.
//!
//! Both goal regions **touch the domain boundary** (A: the grounded `z=0`
//! plate, boundary-clipped windows; B: the driven `z=1` plate), so
//! conductor transfer across prescribed faces is exercised on every level.
//! Both select their region through **3-D tet tags** (`Region::Tag`): a
//! `Region::Box` re-selection is not refinement-stable (bisecting the
//! box's own tets can move every child centroid out of the box and empty
//! the region — the tags, carried by the refinement, define the same
//! physical region on every level).
//!
//! The uniform affine plate (`φ = z`, uncharged) is the zero-error check:
//! its estimate vanishes, the loop stops at level 0, and its effectivity
//! ratio is **undefined** — it does not count as one of the two
//! measurements above.
//!
//! Reading the tables: the estimator linearizes the observable around the
//! **current** winning window, so a level whose winner jumps to a new
//! window (always possible while new centroids appear near a boundary)
//! under-reports the walk part of the error — that is why the location
//! shift is printed with every table and why geometry A's level 0 (a
//! window jump) reports a negative ratio while the levels with a stable
//! winner track the discrepancy.

use std::f64::consts::PI;

use geode_core::adapt::peak_field_loop::{
    PeakFieldAdaptOptions, PeakFieldAdaptSpec, PeakFieldLevelRecord, PeakFieldStopReason,
    adapt_peak_field,
};
use geode_core::assembly::electrostatic::{
    EPS_0, Electrode, assemble_electrostatic, recover_e_field,
};
use geode_core::mesh::{TaggedTetMesh, TetMesh, cube_tet_mesh};
use geode_core::postproc::peak_field::{Region, peak_field};
use geode_core::postproc::sample::Aabb;

type V3 = [f64; 3];

fn centroid(mesh: &TetMesh, tet: &[u32; 4]) -> V3 {
    let mut c = [0.0_f64; 3];
    for &n in tet {
        for (d, cd) in c.iter_mut().enumerate() {
            *cd += mesh.nodes[n as usize][d];
        }
    }
    c.map(|v| v / 4.0)
}

fn in_box(c: &V3, b: &Aabb) -> bool {
    (0..3).all(|d| c[d] >= b.min[d] && c[d] <= b.max[d])
}

/// Tags: [`TAG`] for tets whose centroid lies in `region`, 0 elsewhere.
fn region_tags(mesh: &TetMesh, region: &Aabb, tag: i32) -> Vec<i32> {
    mesh.tets
        .iter()
        .map(|tet| i32::from(in_box(&centroid(mesh, tet), region)) * tag)
        .collect()
}

// ---------------------------------------------------------------------
// Geometry A: nonuniformly loaded plates, φ = sin(πz) (analytic)
// ---------------------------------------------------------------------

/// Goal region of geometry A: a plate-adjacent box **touching the z = 0
/// boundary** (windows centred near its tets are clipped by the plate).
const REGION_A: Aabb = Aabb {
    min: [0.3, 0.3, 0.0],
    max: [0.7, 0.7, 0.2],
};
const WINDOW_A: f64 = 0.2;
/// Physical tag of geometry A's region tets (`Region::Tag` path).
const TAG_A: i32 = 7;

/// Exact potential of geometry A.
fn phi_a(z: f64) -> f64 {
    (PI * z).sin()
}

/// Consistent volume charge `ρ = −ε₀∇²φ = ε₀π²sin(πz)` (C/m³).
fn rho_a(z: f64) -> f64 {
    EPS_0 * PI * PI * (PI * z).sin()
}

/// Analytic limit of geometry A's observable: the domain-clipped window
/// average of `|E| = π|cos(πz)|`. The field is monotone toward either
/// plate, so the supremal window sits flush against `z = 0` and averages
/// `πcos(πz)` over `z ∈ [0, w/2]` — an upper bound for **every** mesh's
/// window average, hence a bias-free reference.
fn j_ref_a() -> f64 {
    let half = WINDOW_A / 2.0;
    (PI * half).sin() / half
}

/// Geometry A problem on `cube_tet_mesh(n)`: both plates grounded, charge
/// sampled per tet at the centroid, region tets tagged [`TAG_A`].
fn geometry_a(n: usize) -> PeakFieldAdaptSpec<'static> {
    let mesh = cube_tet_mesh(n, 1.0);
    let n_tets = mesh.n_tets();
    let ground: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| {
            let z = mesh.nodes[v as usize][2];
            z == 0.0 || z == 1.0
        })
        .collect();
    let rho: Vec<f64> = mesh
        .tets
        .iter()
        .map(|tet| rho_a(centroid(&mesh, tet)[2]))
        .collect();
    let tags = region_tags(&mesh, &REGION_A, TAG_A);
    PeakFieldAdaptSpec {
        mesh: TaggedTetMesh {
            mesh,
            tet_physical_tags: tags,
            ..Default::default()
        },
        eps_r: Box::leak(vec![1.0; n_tets].into_boxed_slice()),
        rho: Box::leak(rho.into_boxed_slice()),
        electrodes: Box::leak(Vec::new().into_boxed_slice()),
        ground: Box::leak(ground.into_boxed_slice()),
        region: Region::Tag(TAG_A),
        window_edge: WINDOW_A,
        e_ref: 1.0,
    }
}

/// Geometry A observable on a uniform mesh with the **analytic** potential
/// sampled at the nodes (same region — by centroid — and window): the
/// mesh-dependence of the discrete family, not a solve. Returns
/// `(value, winning window centre)`.
fn scan_a(n: usize) -> (f64, V3) {
    let mesh = cube_tet_mesh(n, 1.0);
    let phi: Vec<f64> = mesh.nodes.iter().map(|p| phi_a(p[2])).collect();
    let e = recover_e_field(&mesh, &phi);
    let r = peak_field(&mesh, None, &e, &Region::Box(REGION_A), WINDOW_A, 1.0)
        .expect("region A is populated");
    (r.value, r.location)
}

// ---------------------------------------------------------------------
// Geometry B: half-plate Dirichlet–Neumann edge (solved, release tier)
// ---------------------------------------------------------------------

/// Goal region of geometry B: the box `[0.25,0.5]² × [0.75,1]` **touching
/// the driven plate**, hugging the D–N edge line `x = 0.5, z = 1` where
/// the field concentrates. Aligned with the `n = 4` cell faces, so the
/// tagged region is exactly the box on every mesh of the family.
const REGION_B: Aabb = Aabb {
    min: [0.25, 0.25, 0.75],
    max: [0.5, 0.5, 1.0],
};
const WINDOW_B: f64 = 0.1;
const TAG_B: i32 = 7;

/// Geometry B boundary conditions: ground on the whole `z = 0` plate, a
/// half plate at 1 V on `z = 1, x ≤ 0.5`, natural elsewhere. The two
/// Dirichlet groups share no edge. Uncharged, vacuum.
fn bc_b(mesh: &TetMesh) -> (Vec<u32>, Vec<Electrode>) {
    let ground: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| mesh.nodes[v as usize][2] == 0.0)
        .collect();
    let driven: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| {
            let p = mesh.nodes[v as usize];
            p[2] == 1.0 && p[0] <= 0.5
        })
        .collect();
    (
        ground,
        vec![Electrode {
            name: "half".to_string(),
            nodes: driven,
            voltage: 1.0,
        }],
    )
}

fn geometry_b(n: usize) -> PeakFieldAdaptSpec<'static> {
    let mesh = cube_tet_mesh(n, 1.0);
    let n_tets = mesh.n_tets();
    let (ground, electrodes) = bc_b(&mesh);
    let tags = region_tags(&mesh, &REGION_B, TAG_B);
    PeakFieldAdaptSpec {
        mesh: TaggedTetMesh {
            mesh,
            tet_physical_tags: tags,
            ..Default::default()
        },
        eps_r: Box::leak(vec![1.0; n_tets].into_boxed_slice()),
        rho: Box::leak(vec![0.0; n_tets].into_boxed_slice()),
        electrodes: Box::leak(electrodes.into_boxed_slice()),
        ground: Box::leak(ground.into_boxed_slice()),
        region: Region::Tag(TAG_B),
        window_edge: WINDOW_B,
        e_ref: 1.0,
    }
}

/// Geometry B reference: **finer solve** of the same discrete problem on a
/// uniform mesh (the region box aligns with the cells of every `n`
/// divisible by 4, so it is the same physical region). Returns
/// `(value, winning window centre)`.
fn reference_b(n: usize) -> (f64, V3) {
    let mesh = cube_tet_mesh(n, 1.0);
    let n_tets = mesh.n_tets();
    let (ground, electrodes) = bc_b(&mesh);
    let phi = assemble_electrostatic(
        &mesh,
        &vec![1.0; n_tets],
        &vec![0.0; n_tets],
        &electrodes,
        &ground,
    )
    .expect("reference assembly succeeds")
    .solve()
    .expect("reference solve succeeds");
    let e = recover_e_field(&mesh, &phi);
    let r = peak_field(&mesh, None, &e, &Region::Box(REGION_B), WINDOW_B, 1.0)
        .expect("region B is populated");
    (r.value, r.location)
}

// ---------------------------------------------------------------------
// Shared reporting
// ---------------------------------------------------------------------

/// Print one geometry's effectivity table and return the per-level
/// measured discrepancies `J_ref − peak_value`.
fn print_table(name: &str, history: &[PeakFieldLevelRecord], j_ref: f64, ref_loc: &V3) -> Vec<f64> {
    println!("\n== {name}: effectivity table ==");
    println!(
        "reference J_ref = {j_ref:.6} (winning window centre {:.4},{:.4},{:.4})",
        ref_loc[0], ref_loc[1], ref_loc[2]
    );
    println!(
        "{:>5} {:>8} {:>7} {:>8} {:>12} {:>12} {:>12} {:>10}   window centre",
        "level", "dofs", "tets", "marked", "peak", "ref err", "estimate", "eff"
    );
    let mut discrepancies = Vec::with_capacity(history.len());
    for h in history {
        let err_ref = j_ref - h.peak_value;
        let eff = h.estimate / err_ref;
        println!(
            "{:>5} {:>8} {:>7} {:>8} {:>12.6} {:>12.6} {:>12.6} {:>10.4}   {:.4},{:.4},{:.4}",
            h.level,
            h.n_dofs,
            h.n_tets,
            h.n_marked,
            h.peak_value,
            err_ref,
            h.estimate,
            eff,
            h.peak_location[0],
            h.peak_location[1],
            h.peak_location[2],
        );
        discrepancies.push(err_ref);
    }
    let first = &history[0];
    let last = &history[history.len() - 1];
    println!(
        "maximizing-location shift level 0 → {}: ({:+.4},{:+.4},{:+.4}); final vs reference: ({:+.4},{:+.4},{:+.4})",
        last.level,
        last.peak_location[0] - first.peak_location[0],
        last.peak_location[1] - first.peak_location[1],
        last.peak_location[2] - first.peak_location[2],
        last.peak_location[0] - ref_loc[0],
        last.peak_location[1] - ref_loc[1],
        last.peak_location[2] - ref_loc[2],
    );
    discrepancies
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

/// AC 3+4 (first geometry): nonuniformly loaded plates — curved field,
/// nonzero reference error, analytic-limit reference whose uncertainty is
/// the printed scan spread, measured error strictly decreasing under
/// goal-driven refinement, effectivity within a justified band on the
/// levels with a stable winning window.
#[test]
fn charged_plates_effectivity() {
    let spec = geometry_a(4);
    let options = PeakFieldAdaptOptions {
        tolerance: 0.0,
        max_levels: 4,
        max_dofs: 60_000,
        theta: 0.5,
        ..Default::default()
    };
    let result = adapt_peak_field(&spec, &options).expect("loop succeeds");

    let j_ref = j_ref_a();
    let (scan_coarse, _) = scan_a(24);
    let (scan_fine, ref_loc) = scan_a(32);
    let sensitivity = (scan_fine - scan_coarse).abs();
    println!(
        "A reference: analytic limit = {j_ref:.6}; analytic-sampled scans J(24) = {scan_coarse:.6}, J(32) = {scan_fine:.6}, spread = {sensitivity:.3e}"
    );
    let discrepancies = print_table(
        "A charged plates (φ = sin πz)",
        &result.history,
        j_ref,
        &ref_loc,
    );
    println!("stop reason: {}", result.stop_reason.name());

    // The solved levels respect the DOF budget.
    for h in &result.history {
        assert!(
            h.n_dofs <= options.max_dofs,
            "level {} exceeded budget",
            h.level
        );
    }
    // A real adaptive run: several levels, nonzero estimates and errors.
    assert!(
        result.history.len() >= 3,
        "need reported levels to study effectivity"
    );
    for h in &result.history {
        assert!(
            h.estimate.abs() > 0.0,
            "charged problem has nonzero estimate"
        );
        assert!(
            h.peak_value < j_ref,
            "monotone plate field bounds every window average"
        );
    }

    // Reference uncertainty small relative to every discrepancy used in
    // the ratios.
    for (l, &d) in discrepancies.iter().enumerate() {
        assert!(
            sensitivity <= 0.1 * d.abs(),
            "reference sensitivity {sensitivity:.3e} not << discrepancy {d:.3e} at level {l}"
        );
    }

    // Goal-driven refinement reduces the measured error across the
    // reported levels (strictly decreasing).
    for &d in &discrepancies[..discrepancies.len() - 1] {
        assert!(
            discrepancies.last().unwrap().abs() < d.abs(),
            "final error {:?} must drop below earlier {d:?}",
            discrepancies.last()
        );
    }

    // Justified effectivity range on the levels with a stable winner
    // (level ≥ 1 here): positive and within a factor ~3 of the measured
    // 0.31–0.42 — the walk-free linearization tracks the discrepancy.
    for h in &result.history[1..] {
        let eff = h.effectivity(j_ref);
        assert!(eff.is_finite(), "effectivity must be finite: {eff}");
        assert!(
            eff > 0.0,
            "effectivity sign wrong: {eff} at level {}",
            h.level
        );
        assert!(
            (0.15..0.9).contains(&eff),
            "effectivity {eff} outside justified range at level {}",
            h.level
        );
    }
}

/// AC 3+4 (second geometry): half-plate D–N edge problem — consistent PDE
/// and boundary data, `|E| ∝ r^(−1/2)` edge enhancement at `x = 0.5,
/// z = 1`, finer-solve reference (n = 32, sensitivity n = 40), nonzero
/// errors, strictly decreasing under goal-driven refinement, positive
/// effectivity on the levels with a stable winner.
///
/// `#[cfg_attr(debug_assertions, ignore)]`: the two reference solves
/// (n = 32 and n = 40, ~40 k and ~68 k unknowns) plus the effectivity
/// levels are release-tier only, like `cube_convergence_regression`
/// (issue #813); CI runs this target with `--include-ignored` in release.
#[cfg_attr(debug_assertions, ignore)]
#[test]
fn edge_problem_effectivity() {
    let spec = geometry_b(4);
    let options = PeakFieldAdaptOptions {
        tolerance: 0.0,
        max_levels: 4,
        max_dofs: 60_000,
        theta: 0.5,
        ..Default::default()
    };
    let result = adapt_peak_field(&spec, &options).expect("loop succeeds");

    let (j_ref, ref_loc) = reference_b(32);
    let (j_sens, _) = reference_b(40);
    let sensitivity = (j_sens - j_ref).abs();
    println!(
        "B reference: J(n=32) = {j_ref:.6}, J(n=40) = {j_sens:.6}, sensitivity = {sensitivity:.3e}"
    );
    let discrepancies = print_table(
        "B half-plate edge (ground z=0, 1V x≤0.5)",
        &result.history,
        j_ref,
        &ref_loc,
    );
    println!("stop reason: {}", result.stop_reason.name());

    for h in &result.history {
        assert!(
            h.n_dofs <= options.max_dofs,
            "level {} exceeded budget",
            h.level
        );
    }
    assert!(
        result.history.len() >= 3,
        "need reported levels to study effectivity"
    );
    for h in &result.history {
        assert!(
            h.peak_value < j_ref,
            "adaptive family stays below its finer reference"
        );
    }

    // Reference uncertainty small relative to every measured discrepancy.
    for (l, &d) in discrepancies.iter().enumerate() {
        assert!(
            sensitivity <= 0.1 * d.abs(),
            "reference sensitivity {sensitivity:.3e} not << discrepancy {d:.3e} at level {l}"
        );
    }

    // Convergence criterion (AC 4): the measured error strictly decreases
    // across the reported levels.
    for &d in &discrepancies[..discrepancies.len() - 1] {
        assert!(
            discrepancies.last().unwrap().abs() < d.abs(),
            "final error {:?} must drop below earlier {d:?}",
            discrepancies.last()
        );
    }

    // Effectivity on the levels with a stable winner (level ≥ 2 here):
    // positive and within the justified band around the measured
    // 0.12–0.13. The ratio is well below 1 — the D–N edge keeps pulling
    // the winner toward the plate, so part of the measured discrepancy is
    // the walk the fixed-window linearization cannot see; the printed
    // location-shift line quantifies exactly that.
    for h in &result.history[2..] {
        let eff = h.effectivity(j_ref);
        assert!(eff.is_finite(), "effectivity must be finite: {eff}");
        assert!(
            eff > 0.0,
            "effectivity sign wrong: {eff} at level {}",
            h.level
        );
        assert!(
            (0.05..0.35).contains(&eff),
            "effectivity {eff} outside justified range at level {}",
            h.level
        );
    }
}

/// AC 4 (zero-error check) + Test Plan "tolerance met on the initial
/// mesh": the uniform affine plate solves exactly, the estimate vanishes,
/// the loop stops on the tolerance at level 0 and its ratio is undefined
/// — this is NOT one of the two measurements.
#[test]
fn affine_plate_is_zero_error_and_stops_at_level0() {
    let mesh = cube_tet_mesh(4, 1.0);
    let n_tets = mesh.n_tets();
    let ground: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| mesh.nodes[v as usize][2] == 0.0)
        .collect();
    let top: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| mesh.nodes[v as usize][2] == 1.0)
        .collect();
    let spec = PeakFieldAdaptSpec {
        mesh: TaggedTetMesh {
            mesh,
            tet_physical_tags: vec![0; n_tets],
            ..Default::default()
        },
        eps_r: &vec![1.0; n_tets],
        rho: &vec![0.0; n_tets],
        electrodes: &[Electrode {
            name: "top".to_string(),
            nodes: top,
            voltage: 1.0,
        }],
        ground: &ground,
        region: Region::Box(Aabb {
            min: [0.0; 3],
            max: [1.0; 3],
        }),
        window_edge: 0.4,
        e_ref: 1.0,
    };
    let options = PeakFieldAdaptOptions {
        tolerance: 1e-9,
        max_levels: 6,
        max_dofs: 100_000,
        ..Default::default()
    };
    let result = adapt_peak_field(&spec, &options).expect("affine loop succeeds");

    println!(
        "affine plate: stop {}, {} level(s), estimate = {:.3e}",
        result.stop_reason.name(),
        result.history.len(),
        result.estimate
    );
    assert_eq!(result.stop_reason, PeakFieldStopReason::ToleranceMet);
    assert_eq!(result.history.len(), 1, "tolerance met on the initial mesh");
    assert_eq!(result.history[0].n_marked, 0);
    assert!(
        result.estimate.abs() <= 1e-9 * result.peak.value,
        "affine estimate {} must vanish (peak {})",
        result.estimate,
        result.peak.value
    );
    // φ = z is reproduced exactly: the observable is the plate field 1
    // and the reference discrepancy vanishes — the ratio is undefined.
    assert!((result.peak.value - 1.0).abs() <= 1e-9);
}

/// Test Plan: a primal DOF budget that prevents the next conforming
/// refinement — the loop stops with `dof_budget`, retains the last solved
/// level (its mesh is the initial one, within budget) and reports the
/// marking it attempted.
#[test]
fn dof_budget_stops_and_retains_last_solved_level() {
    let spec = geometry_a(4);
    let n0 = spec.mesh.mesh.n_nodes();
    let options = PeakFieldAdaptOptions {
        tolerance: 0.0,
        max_levels: 12,
        // The initial mesh fits exactly; any conforming refinement of the
        // marked set (conformity closure included) adds nodes.
        max_dofs: n0,
        theta: 0.5,
        ..Default::default()
    };
    let result = adapt_peak_field(&spec, &options).expect("loop succeeds");

    println!(
        "budget run: stop {}, levels {}, initial dofs {n0}, final dofs {}",
        result.stop_reason.name(),
        result.history.len(),
        result.mesh.mesh.n_nodes()
    );
    assert_eq!(result.stop_reason, PeakFieldStopReason::DofBudget);
    assert_eq!(result.history.len(), 1);
    assert!(result.history[0].n_marked > 0, "a marking was attempted");
    // The last solved level is retained: its mesh is the initial mesh.
    assert_eq!(result.mesh.mesh.n_nodes(), n0);
    assert_eq!(result.history[0].n_dofs, n0);
    assert_eq!(result.phi.len(), n0);
}

// ---------------------------------------------------------------------
// Conductor-node transfer membership (issue #1785)
// ---------------------------------------------------------------------

/// The pre-#1785 transfer scan (column rescan per new node), kept here as
/// the differential oracle of
/// [`transfer_conductor_nodes`]'s shared-support rewrite: the two must
/// produce identical node sets.
fn transfer_scan_oracle(
    coarse: &TetMesh,
    refined: &geode_core::adapt::refine::Refined,
    group: &[u32],
) -> std::collections::BTreeSet<u32> {
    use geode_core::adapt::electrostatic_p1::prescribed_faces;
    use std::collections::BTreeSet;
    let fine = &refined.mesh.mesh;
    let mut nodes: BTreeSet<u32> = group.iter().copied().collect();
    if group.is_empty() {
        return nodes;
    }
    let faces = prescribed_faces(
        coarse,
        &[Electrode {
            name: "transfer".to_string(),
            nodes: group.to_vec(),
            voltage: 0.0,
        }],
        &[],
    );
    let mut edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for f in &faces {
        for i in 0..3 {
            for j in i + 1..3 {
                edges.insert((f[i].min(f[j]), f[i].max(f[j])));
            }
        }
    }
    let p = refined.node_prolongation.as_ref();
    let (cp, ri) = (p.col_ptr(), p.row_idx());
    for i in coarse.n_nodes()..fine.n_nodes() {
        let mut support = [0_u32; 2];
        let mut n = 0;
        'col: for j in 0..p.ncols() {
            for &row in &ri[cp[j]..cp[j + 1]] {
                if row == i {
                    if n == 2 {
                        n = 3;
                        break 'col;
                    }
                    support[n] = j as u32;
                    n += 1;
                }
            }
        }
        if n == 2 {
            let key = (support[0].min(support[1]), support[0].max(support[1]));
            if edges.contains(&key) {
                nodes.insert(i as u32);
            }
        }
    }
    nodes
}

/// Test Plan (issue #1785): transfer membership on a small refinement —
/// an empty ground (the shortcut), two conductor groups adjacent on the
/// `z = 1` plate (each keeps its own nodes and gains the midpoints of its
/// prescribed faces' edges), a **face shared by the two groups** (its
/// edge midpoints join neither group), and the differential oracle above:
/// the shared-support transfer reproduces the per-node scan's exact node
/// sets on every level.
#[test]
fn transfer_conductor_nodes_membership() {
    use geode_core::adapt::electrostatic_p1::prescribed_faces;
    use geode_core::adapt::peak_field_loop::transfer_conductor_nodes;
    use geode_core::adapt::refine::{BisectionMesh, RefineOpts};
    use std::collections::BTreeSet;

    let mesh = cube_tet_mesh(4, 1.0);
    // Two groups adjacent on the z = 1 plate, sharing the seam x = 0.5.
    let left: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| {
            let p = mesh.nodes[v as usize];
            p[2] == 1.0 && p[0] <= 0.5
        })
        .collect();
    let right: Vec<u32> = (0..mesh.n_nodes() as u32)
        .filter(|&v| {
            let p = mesh.nodes[v as usize];
            p[2] == 1.0 && p[0] > 0.5
        })
        .collect();
    let left_set: BTreeSet<u32> = left.iter().copied().collect();
    let right_set: BTreeSet<u32> = right.iter().copied().collect();

    // Two uniform levels through one persistent bisection mesh: the first
    // bisects the interior cell diagonals only, the second the plate
    // edges, so the second level's transfer has actual joins.
    let tagged = TaggedTetMesh {
        tet_physical_tags: vec![0; mesh.n_tets()],
        mesh: mesh.clone(),
        ..Default::default()
    };
    let mut bis = BisectionMesh::new(tagged).expect("bisection mesh");
    let mut prev = mesh.clone();
    let mut joined_any_level = false;
    let mut saw_seam_midpoint = false;
    for _level in 0..2 {
        let marked: Vec<usize> = (0..prev.n_tets()).collect();
        let refined = bis.refine(&marked, &RefineOpts::default()).expect("refine");
        let n_prev = prev.n_nodes();

        // Empty ground: the shortcut returns the (empty) set unchanged.
        assert!(
            transfer_conductor_nodes(&prev, &refined, &[]).is_empty(),
            "empty group transfers to the empty set"
        );

        // Differential oracle: identical node sets to the per-node scan.
        let tl = transfer_conductor_nodes(&prev, &refined, &left);
        let tr = transfer_conductor_nodes(&prev, &refined, &right);
        assert_eq!(tl, transfer_scan_oracle(&prev, &refined, &left), "left");
        assert_eq!(tr, transfer_scan_oracle(&prev, &refined, &right), "right");

        // Old nodes are retained; no node joins both groups.
        assert!(tl.is_superset(&left_set) && tr.is_superset(&right_set));
        assert!(
            tl.intersection(&tr).next().is_none(),
            "a node joined two conductor groups"
        );

        // Every element is the group's own (old) node or a NEW node — the
        // old nodes' one-entry support rows are skipped — and every joined
        // node has exactly two support entries (a plain edge midpoint).
        let p = refined.node_prolongation.as_ref();
        let (cp, ri) = (p.col_ptr(), p.row_idx());
        let mut support: Vec<Vec<u32>> = vec![Vec::new(); refined.mesh.mesh.n_nodes()];
        for j in 0..p.ncols() {
            for k in cp[j]..cp[j + 1] {
                support[ri[k]].push(j as u32);
            }
        }
        for (set, group) in [(&tl, &left_set), (&tr, &right_set)] {
            for &v in set {
                assert!(
                    group.contains(&v) || v as usize >= n_prev,
                    "a transfer node is neither an old group node nor a new node"
                );
            }
            for &v in set.iter().filter(|v| (**v as usize) >= n_prev) {
                assert_eq!(
                    support[v as usize].len(),
                    2,
                    "a joined node without a two-entry (edge midpoint) support"
                );
            }
        }

        // Faces mixing the two groups are prescribed for neither: no face
        // of either group's prescribed set spans both groups, and a new
        // midpoint of a seam edge (one endpoint per group) joins no group.
        let left_faces = prescribed_faces(
            &prev,
            &[Electrode {
                name: "left".to_string(),
                nodes: left.clone(),
                voltage: 0.0,
            }],
            &[],
        );
        let right_faces = prescribed_faces(
            &prev,
            &[Electrode {
                name: "right".to_string(),
                nodes: right.clone(),
                voltage: 0.0,
            }],
            &[],
        );
        for f in left_faces.iter().chain(right_faces.iter()) {
            assert!(
                !(f.iter().any(|v| left_set.contains(v))
                    && f.iter().any(|v| right_set.contains(v))),
                "a face mixing both conductor groups is prescribed"
            );
        }
        let mut seam = 0;
        for (i, sup) in support.iter().enumerate().skip(n_prev) {
            if let [a, b] = sup.as_slice()
                && (left_set.contains(a) && right_set.contains(b)
                    || right_set.contains(a) && left_set.contains(b))
            {
                seam += 1;
                saw_seam_midpoint = true;
                assert!(
                    !tl.contains(&(i as u32)) && !tr.contains(&(i as u32)),
                    "a mixed-group seam midpoint joined a conductor group"
                );
            }
        }
        if tl.len() > left.len() {
            joined_any_level = true;
        }
        if seam > 0 {
            println!(
                "transfer level {_level}: left +{} right +{} seam midpoints {seam}",
                tl.len() - left.len(),
                tr.len() - right.len()
            );
        }
        prev = refined.mesh.mesh.clone();
    }
    assert!(joined_any_level, "no new node ever joined a group");
    assert!(
        saw_seam_midpoint,
        "no mixed-group seam midpoint was refined"
    );
}
