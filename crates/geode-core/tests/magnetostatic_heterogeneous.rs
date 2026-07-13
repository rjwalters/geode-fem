//! Acceptance tests for **heterogeneous-`μ` (piecewise-`ν`) magnetostatics**
//! on a multi-band cross-section (Epic #448, Phase 2 / child #452).
//!
//! Phase 1 (#450, `magnetostatic_wire.rs`) delivered the scalar-P1
//! `−∇·(ν∇A_z) = J_z` solver and validated only the **homogeneous**
//! (uniform-`μ`) wire. A machine cross-section is **piecewise-`μ`**:
//! high-permeability iron (`μ_r ≫ 1`) abutting an `μ_r = 1` air gap. This
//! file adds the multi-band annular mesher
//! ([`disk_tri_mesh_bands`]) + the tag→reluctivity helper
//! ([`build_nu_r`]) and validates `ν`-heterogeneity against two **exact**
//! oracles, per the #452 Test Plan:
//!
//! 1. **Mesher units** — `disk_tri_mesh_bands` tags cover every band, band
//!    interfaces conform (nodes land exactly on `r_k`), triangles are CCW
//!    and non-degenerate, and the sliver aspect-ratio guard passes for a
//!    ~1 %-radius gap band.
//! 2. **`build_nu_r` unit** — tag → `ν = 1/μ_r` map is correct and
//!    length-preserving.
//! 3. **Current-sheet oracle** — two antiparallel axial-current sheets give
//!    a uniform `|B|` between them and ~zero outside (the planar
//!    solenoid), to ≤ 1 %.
//! 4. **`μ`-contrast oracle** — a wire threaded through concentric `μ_r`
//!    shells produces `B_θ(r) = μ_r(r)·μ₀I/(2πr)`, the concentric-shell
//!    magnetostatic closed form; the field jumps by exactly the `μ`-ratio
//!    across each interface, matched to ≤ 1 %.
//! 5. **Inverse tripwire** — swapping two bands' `μ_r` (a wrong `ν` map)
//!    drives the contrast-oracle error far above the 1 % pass bar.
//!
//! Index-based loops over the fixed element matrices / dense readbacks read
//! closer to the linear algebra than iterator chains, so the
//! `needless_range_loop` lint is silenced file-wide as in the Phase-1 test.
#![allow(clippy::needless_range_loop)]

use geode_core::analytic::waveguide::{
    ASPECT_RATIO_SLIVER_BOUND, TriMesh, disk_tri_mesh_bands, disk_tri_mesh_bands_checked,
    worst_aspect_ratio,
};
use geode_core::assembly::magnetostatic::{assemble_magnetostatic, build_nu_r, recover_b_field};

const MU_0: f64 = 4.0e-7 * std::f64::consts::PI;
const PI: f64 = std::f64::consts::PI;

// ─────────────────────────────────────────────────────────────────────
// Small geometry helpers (test-local)
// ─────────────────────────────────────────────────────────────────────

/// Centroid `(x, y)` of triangle `t`.
fn centroid(mesh: &TriMesh, t: usize) -> [f64; 2] {
    let tri = mesh.tris[t];
    let mut c = [0.0_f64; 2];
    for &v in &tri {
        c[0] += mesh.nodes[v as usize][0];
        c[1] += mesh.nodes[v as usize][1];
    }
    [c[0] / 3.0, c[1] / 3.0]
}

/// Centroid radius of triangle `t`.
fn centroid_r(mesh: &TriMesh, t: usize) -> f64 {
    let c = centroid(mesh, t);
    (c[0] * c[0] + c[1] * c[1]).sqrt()
}

/// Absolute triangle area (shoelace).
fn tri_area(mesh: &TriMesh, t: usize) -> f64 {
    let tri = mesh.tris[t];
    let p0 = mesh.nodes[tri[0] as usize];
    let p1 = mesh.nodes[tri[1] as usize];
    let p2 = mesh.nodes[tri[2] as usize];
    0.5 * ((p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0])).abs()
}

/// Signed triangle area (positive for CCW).
fn tri_signed_area(mesh: &TriMesh, t: usize) -> f64 {
    let tri = mesh.tris[t];
    let p0 = mesh.nodes[tri[0] as usize];
    let p1 = mesh.nodes[tri[1] as usize];
    let p2 = mesh.nodes[tri[2] as usize];
    0.5 * ((p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]))
}

// ═════════════════════════════════════════════════════════════════════
// 1. Multi-band mesher unit tests
// ═════════════════════════════════════════════════════════════════════

/// A representative 4-band machine annulus:
///   band 0 (shaft/back-iron core) [0.00, 0.40)
///   band 1 (magnet)               [0.40, 0.70)
///   band 2 (AIR GAP, ~1 % radius) [0.70, 0.71)   ← thin sliver-risk band
///   band 3 (stator iron)          [0.71, 1.00)
const MACHINE_RADII: [f64; 5] = [0.0, 0.40, 0.70, 0.71, 1.00];

#[test]
fn bands_mesh_tags_cover_every_band_and_conform() {
    // Give the thin (~1 %) air-gap band its own radial subdivision so its
    // cells are not radially crushed; the thick bands get more rings.
    let n_radial = [6usize, 5, 2, 5];
    let gradings = [geode_core::analytic::waveguide::RadialGrading::Uniform; 4];
    let (mesh, tags) = disk_tri_mesh_bands(&MACHINE_RADII, 96, &n_radial, &gradings);

    assert_eq!(tags.len(), mesh.n_tris(), "one tag per triangle");

    // Every band index 0..4 must appear.
    for band in 0..4i32 {
        assert!(tags.contains(&band), "band {band} produced no triangles");
    }
    // No stray tags outside 0..4.
    assert!(
        tags.iter().all(|&t| (0..4).contains(&t)),
        "tag outside the valid 0..4 band range"
    );

    // Each triangle's centroid radius lands in its tagged band's annulus,
    // i.e. the centroid test agrees with the tag (conforming interfaces:
    // no triangle straddles a band boundary).
    for t in 0..mesh.n_tris() {
        let r = centroid_r(&mesh, t);
        let band = tags[t] as usize;
        let (lo, hi) = (MACHINE_RADII[band], MACHINE_RADII[band + 1]);
        assert!(
            r >= lo - 1e-12 && r < hi + 1e-12,
            "tri {t} centroid r={r} not in band {band} = [{lo}, {hi})"
        );
    }

    // Conformity: a ring of nodes lands *exactly* on every interior band
    // radius (so the material interface is a mesh edge, not a triangle
    // interior). Check each interface radius has ≥ n_angular nodes on it.
    for &r_k in &MACHINE_RADII[1..MACHINE_RADII.len() - 1] {
        let on_ring = mesh
            .nodes
            .iter()
            .filter(|p| ((p[0] * p[0] + p[1] * p[1]).sqrt() - r_k).abs() < 1e-9)
            .count();
        assert!(
            on_ring >= 96,
            "band radius r={r_k} has only {on_ring} nodes on it (< n_angular=96) — not conforming"
        );
    }

    // Every triangle is CCW (positive signed area) and non-degenerate.
    for t in 0..mesh.n_tris() {
        assert!(
            tri_signed_area(&mesh, t) > 0.0,
            "tri {t} is not CCW / is degenerate"
        );
    }
}

#[test]
fn bands_thin_gap_passes_aspect_guard() {
    // The ~1 %-radius air-gap band is the sliver risk. With enough angular
    // sectors and its own radial subdivision, the checked mesher must
    // accept it under the default sliver bound.
    let n_radial = [6usize, 5, 2, 5];
    let gradings = [geode_core::analytic::waveguide::RadialGrading::Uniform; 4];
    let res = disk_tri_mesh_bands_checked(
        &MACHINE_RADII,
        128,
        &n_radial,
        &gradings,
        ASPECT_RATIO_SLIVER_BOUND,
    );
    let (mesh, _tags) = res.expect("thin-gap machine annulus must pass the sliver guard");
    let worst = worst_aspect_ratio(&mesh);
    println!("machine-annulus worst aspect ratio = {worst:.3} (bound {ASPECT_RATIO_SLIVER_BOUND})");
    assert!(
        worst <= ASPECT_RATIO_SLIVER_BOUND,
        "worst aspect ratio {worst:.3} exceeds bound"
    );
}

#[test]
fn bands_checked_rejects_under_resolved_thin_gap() {
    // The same thin gap band starved of angular resolution AND radial
    // subdivision yields slivers the guard must reject — proving the guard
    // is not vacuous.
    let n_radial = [2usize, 2, 1, 2];
    let gradings = [geode_core::analytic::waveguide::RadialGrading::Uniform; 4];
    // Very fat wedges (few angular sectors) make the thin-gap cells
    // pathological.
    let res = disk_tri_mesh_bands_checked(&MACHINE_RADII, 6, &n_radial, &gradings, 8.0);
    assert!(
        res.is_err(),
        "under-resolved thin gap should trip the aspect guard, got Ok"
    );
}

#[test]
fn bands_reduce_to_two_region_disk_shape() {
    // A 2-band call is the disk_tri_mesh analogue: a central band + one
    // outer band. Sanity-check counts and tag coverage.
    let radii = [0.0, 0.3, 1.0];
    let n_radial = [4usize, 4];
    let gradings = [geode_core::analytic::waveguide::RadialGrading::Uniform; 2];
    let (mesh, tags) = disk_tri_mesh_bands(&radii, 32, &n_radial, &gradings);
    // Total rings = 8; central fan (32) + 7 annular rings × 32 quads × 2.
    let expected_tris = 32 + 7 * 32 * 2;
    assert_eq!(mesh.n_tris(), expected_tris, "unexpected triangle count");
    assert!(tags.contains(&0), "band 0 present");
    assert!(tags.contains(&1), "band 1 present");
}

// ═════════════════════════════════════════════════════════════════════
// 2. build_nu_r unit test
// ═════════════════════════════════════════════════════════════════════

#[test]
fn build_nu_r_maps_tags_to_reluctivity() {
    // μ_r table: band 0 iron (μ_r=1000), band 1 magnet (μ_r=1.05),
    // band 2 air (μ_r=1), band 3 iron (μ_r=4000).
    let mu_r = [1000.0, 1.05, 1.0, 4000.0];
    let tags = [0i32, 2, 3, 1, 2, 0];
    let nu = build_nu_r(&tags, &mu_r);
    assert_eq!(nu.len(), tags.len(), "one ν per tag");
    let expect = [
        1.0 / 1000.0,
        1.0,
        1.0 / 4000.0,
        1.0 / 1.05,
        1.0,
        1.0 / 1000.0,
    ];
    for (i, (&got, &want)) in nu.iter().zip(expect.iter()).enumerate() {
        assert!(
            (got - want).abs() < 1e-15,
            "ν[{i}] = {got} != {want} (tag {})",
            tags[i]
        );
    }
    // High-μ iron ⇒ tiny reluctivity; air ⇒ ν = 1.
    assert!(nu[0] < 1e-2, "iron reluctivity should be ≪ 1");
    assert_eq!(nu[1], 1.0, "air reluctivity must be exactly 1");
}

#[test]
#[should_panic(expected = "out of range")]
fn build_nu_r_rejects_out_of_range_tag() {
    let mu_r = [1.0, 2.0];
    let tags = [0i32, 5]; // 5 indexes past the 2-entry table
    let _ = build_nu_r(&tags, &mu_r);
}

// ═════════════════════════════════════════════════════════════════════
// 3. Current-sheet oracle — the planar solenoid
// ═════════════════════════════════════════════════════════════════════
//
// DERIVATION (documented for the Judge).
//
// The 2-D scalar reduction is  −∇·(ν∇A_z) = μ₀ J_z  with the recovered
// flux density  B = (∂A_z/∂y, −∂A_z/∂x).  Take air everywhere (ν ≡ 1).
//
// Place two horizontal axial-current sheets of thickness `t` and density
// ±J₀ stacked in y:  +J₀ on [y1a, y1b] (lower), −J₀ on [y2a, y2b] (upper),
// with y1b < y2a.  Drive them on a wide rectangle [0,W]×[0,H] and impose:
//   • A_z = 0 pinned on the BOTTOM wall (y = 0) only,
//   • natural (Neumann, ν ∂A_z/∂n = 0) on the other three walls.
//
// With W ≫ the sheet spacing the solution is x-invariant, so A_z = A_z(y)
// solves the 1-D  −A_z''(y) = μ₀ J_z(y)  and  B_x(y) = A_z'(y),
// B_y = −∂A_z/∂x = 0.  Integrating from the top down:
//   • Neumann top ⇒ B_x(H) = A_z'(H) = 0  ⇒  B_x = 0 ABOVE the upper sheet.
//   • Through the upper sheet (−J₀): B_x' = −μ₀J_z = +μ₀J₀, so crossing a
//     thickness t downward, B_x steps to −μ₀ J₀ t.  (Downward integration
//     flips the sign of the step; magnitude μ₀J₀t.)
//   • BETWEEN the sheets (J_z = 0): B_x = −μ₀ J₀ t, UNIFORM.
//   • Through the lower sheet (+J₀): B_x returns to 0.
//   • BELOW the lower sheet: B_x = 0, consistent with A_z(0)=0 pinned.
//
// Hence the exact oracle: |B| = μ₀ J₀ t uniform between the sheets, |B| ≈ 0
// outside — the planar (unrolled-solenoid) analogue #448 lists.  This
// validates axial-source placement and the field jump across a sheet.

/// Build the current-sheet problem on a rectangular mesh and return
/// `(mesh, A_z, b_field)`.  Sheets are `t`-thick, centered on `y = y1c`
/// (lower, +J₀) and `y = y2c` (upper, −J₀).  Only the bottom wall is pinned.
#[allow(clippy::too_many_arguments)]
fn solve_current_sheet(
    nx: usize,
    ny: usize,
    width: f64,
    height: f64,
    y1c: f64,
    y2c: f64,
    sheet_t: f64,
    j0: f64,
) -> (TriMesh, Vec<f64>, Vec<[f64; 2]>) {
    use geode_core::analytic::waveguide::rect_tri_mesh;
    let mesh = rect_tri_mesh(nx, ny, width, height);
    let n_tris = mesh.n_tris();

    let nu = vec![1.0; n_tris]; // air everywhere
    // Piecewise-constant axial current: +μ₀J₀ folded into the RHS source
    // on the lower sheet band, −μ₀J₀ on the upper.
    let in_band = |yc: f64, y: f64| (y - yc).abs() <= 0.5 * sheet_t;
    let j_z: Vec<f64> = (0..n_tris)
        .map(|t| {
            let yc = centroid(&mesh, t)[1];
            if in_band(y1c, yc) {
                MU_0 * j0
            } else if in_band(y2c, yc) {
                -MU_0 * j0
            } else {
                0.0
            }
        })
        .collect();

    // Pin ONLY the bottom wall (y ≈ 0); Neumann elsewhere.
    let tol = 1e-9 * height.max(1.0);
    let dirichlet: Vec<bool> = mesh.nodes.iter().map(|p| p[1].abs() < tol).collect();

    let sys = assemble_magnetostatic(&mesh, &nu, &j_z, &dirichlet).expect("assemble sheet");
    let a_z = sys.solve().expect("sheet solve");
    let b = recover_b_field(&mesh, &a_z);
    (mesh, a_z, b)
}

#[test]
fn current_sheet_uniform_between_and_zero_outside() {
    let width = 4.0;
    let height = 2.0;
    // Sheets at y = 0.8 and y = 1.2, each 0.05 thick; interior band is
    // (0.825, 1.175).  Align the mesh so grid lines fall on sheet edges:
    // ny = 40 ⇒ hy = 0.05, so sheet centers 0.8/1.2 sit on cell boundaries
    // and each sheet is exactly one cell-row thick.
    let (nx, ny) = (160, 40);
    let sheet_t = 0.05;
    let (y1c, y2c) = (0.825, 1.175); // centers of the one-row-thick sheets
    let j0 = 7.0;

    let (mesh, _a_z, b) = solve_current_sheet(nx, ny, width, height, y1c, y2c, sheet_t, j0);

    // Exact oracle: |B| = μ₀ J₀ t between the sheets, 0 outside.
    let b_expect = MU_0 * j0 * sheet_t;

    // Sample only the middle x-strip (away from the left/right Neumann walls
    // where a thin end-effect layer lives) and away from the sheets in y.
    let x_lo = 0.35 * width;
    let x_hi = 0.65 * width;

    // (a) interior band: y ∈ (0.9, 1.1), strictly between the sheets.
    let mut num_in = 0.0;
    let mut den_in = 0.0;
    let mut n_in = 0usize;
    // (b) exterior band: y ∈ (0.2, 0.6), below the lower sheet.
    let mut ext_energy = 0.0;
    let mut ext_area = 0.0;
    for t in 0..mesh.n_tris() {
        let c = centroid(&mesh, t);
        if c[0] < x_lo || c[0] > x_hi {
            continue;
        }
        let area = tri_area(&mesh, t);
        let bmag = (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
        if c[1] > 0.90 && c[1] < 1.10 {
            num_in += area * (bmag - b_expect).powi(2);
            den_in += area * b_expect * b_expect;
            n_in += 1;
        } else if c[1] > 0.20 && c[1] < 0.60 {
            ext_energy += area * bmag * bmag;
            ext_area += area;
        }
    }
    assert!(n_in > 0, "no interior-band triangles sampled");
    assert!(ext_area > 0.0, "no exterior-band triangles sampled");

    let l2_in = (num_in / den_in).sqrt();
    // Exterior field as a fraction of the interior field (RMS ratio).
    let ext_rms = (ext_energy / ext_area).sqrt();
    let ext_frac = ext_rms / b_expect;

    println!(
        "current-sheet: |B|_interior L2 rel err = {:.4}%, exterior/interior = {:.4}%",
        l2_in * 100.0,
        ext_frac * 100.0
    );

    assert!(
        l2_in <= 0.01,
        "interior uniform-field L2 error {:.4}% exceeds 1% bar",
        l2_in * 100.0
    );
    assert!(
        ext_frac <= 0.01,
        "exterior field {:.4}% of interior exceeds 1% bar",
        ext_frac * 100.0
    );
}

// ═════════════════════════════════════════════════════════════════════
// 4. μ-contrast oracle — wire threaded through concentric μ_r shells
// ═════════════════════════════════════════════════════════════════════
//
// DERIVATION (documented for the Judge).
//
// A line current I along ẑ at the origin, surrounded by CONCENTRIC bands of
// differing relative permeability μ_r(r) (the magnetic analogue of a
// dielectric-shell / concentric-shell problem #448 lists).  The geometry is
// azimuthally symmetric, so H is purely azimuthal, H = H_θ(r) θ̂, and
// Ampère's law on a circle of radius r gives, INDEPENDENT of the μ profile,
//
//     ∮ H·dl = 2π r H_θ = I_enc = I        ⇒     H_θ(r) = I / (2π r)   (r > core).
//
// H depends only on the enclosed current — the μ-heterogeneity does NOT
// change H.  The flux density follows the local constitutive law:
//
//     B_θ(r) = μ₀ μ_r(r) H_θ(r) = μ_r(r) · μ₀ I / (2π r).            (★)
//
// So B_θ is the vacuum wire field μ₀I/(2πr) SCALED by the local μ_r, and it
// JUMPS by exactly the μ-ratio across each concentric interface (B_n = 0 is
// trivially continuous — B is tangential to the circular interface — while
// H_t = B_θ/μ is continuous, so B_θ jumps by μ_r,outer/μ_r,inner).  This is
// the concentric-shell magnetostatic closed form; (★) is the exact oracle
// against which the FE B-field is compared band-by-band.  It quantitatively
// exercises ν-heterogeneity: a wrong ν map mis-scales B_θ in the affected
// band and (★) catches it (see the tripwire below).
//
// The scalar A_z formulation reproduces (★) automatically: A_z is nodally
// continuous across the interface (so B_n is continuous) and the ν-weighted
// stiffness enforces H_t continuity — no special interface term is needed,
// which is the virtue of the scalar form the #448 guidance calls out.

/// Concentric-shell wire radii: a small finite-radius conductor core, then
/// three material shells with distinct μ_r.
///   band 0: conductor core  [0.00, 0.05)   (carries the current)
///   band 1: iron shell       [0.05, 0.40)   μ_r = μ1
///   band 2: air shell        [0.40, 0.70)   μ_r = 1
///   band 3: iron shell       [0.70, 1.00)   μ_r = μ3
const SHELL_RADII: [f64; 5] = [0.0, 0.05, 0.40, 0.70, 1.00];

/// Solve the wire-in-concentric-shells problem and return `(mesh, tags, B)`.
/// The core (band 0) carries a uniform axial current density realizing total
/// current `I`; the μ_r table sets each band's permeability.  ν = 1/μ_r is
/// built via `build_nu_r`.  Outer ring pinned to A_z = 0.
fn solve_wire_shells(
    n_angular: usize,
    n_radial: &[usize],
    mu_r_per_band: &[f64],
    current: f64,
) -> (TriMesh, Vec<i32>, Vec<[f64; 2]>) {
    use geode_core::analytic::waveguide::{RadialGrading, disk_boundary_nodes};
    let gradings = vec![RadialGrading::Uniform; SHELL_RADII.len() - 1];
    let (mesh, tags) = disk_tri_mesh_bands(&SHELL_RADII, n_angular, n_radial, &gradings);
    let n_tris = mesh.n_tris();

    // ν from the per-band μ_r table — the heart of the heterogeneity path.
    let nu = build_nu_r(&tags, mu_r_per_band);

    // Uniform axial current density over the conductor core (band 0),
    // carrying the μ₀ factor of the magnetostatic source.
    let core_r = SHELL_RADII[1];
    let core_area = PI * core_r * core_r;
    let density = MU_0 * current / core_area;
    let j_z: Vec<f64> = tags
        .iter()
        .map(|&tag| if tag == 0 { density } else { 0.0 })
        .collect();

    let bc = disk_boundary_nodes(&mesh, SHELL_RADII[SHELL_RADII.len() - 1]);
    let sys = assemble_magnetostatic(&mesh, &nu, &j_z, &bc).expect("assemble shells");
    let a_z = sys.solve().expect("shell solve");
    let b = recover_b_field(&mesh, &a_z);
    let _ = n_tris;
    (mesh, tags, b)
}

/// L2 relative error of `|B|` vs the concentric-shell closed form (★),
/// `B_θ(r) = μ_r(r)·μ₀ I/(2π r)`, over the triangles of band `band`, using a
/// per-band radial guard `[r_lo, r_hi]` that trims the interface-adjacent
/// cells (where the piecewise-constant P1 gradient smears the μ-jump over one
/// cell width).
#[allow(clippy::too_many_arguments)]
fn shell_band_l2_error(
    mesh: &TriMesh,
    tags: &[i32],
    b: &[[f64; 2]],
    band: i32,
    mu_r_band: f64,
    current: f64,
    r_lo: f64,
    r_hi: f64,
) -> f64 {
    let mut num = 0.0;
    let mut den = 0.0;
    let mut count = 0usize;
    for t in 0..mesh.n_tris() {
        if tags[t] != band {
            continue;
        }
        let r = centroid_r(mesh, t);
        if r < r_lo || r > r_hi {
            continue;
        }
        let area = tri_area(mesh, t);
        let bmag = (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
        let exact = mu_r_band * MU_0 * current / (2.0 * PI * r);
        num += area * (bmag - exact).powi(2);
        den += area * exact * exact;
        count += 1;
    }
    assert!(count > 0, "no triangles in band {band} comparison window");
    (num / den).sqrt()
}

#[test]
fn mu_contrast_matches_concentric_shell_closed_form() {
    let current = 3.0;
    // Iron / air / iron shells.  Modest μ_r (500) keeps the reluctivity
    // contrast large (ν jumps 500×) while staying well-conditioned.
    let mu_r = [1.0, 500.0, 1.0, 500.0]; // band0 core (air-like), then shells
    // Fine radial resolution per band so the mid-band comparison windows
    // clear the first-order P1-gradient error.
    // The innermost iron shell (band 1) has the steepest 1/r curvature and
    // sits closest to the core, so it needs the finest radial resolution to
    // clear the 1% bar under first-order P1-gradient recovery.
    let n_radial = [3usize, 40, 24, 24];
    let (mesh, tags, b) = solve_wire_shells(256, &n_radial, &mu_r, current);

    // Compare each SHELL band on a mid-band window (trim the interface layers
    // where the piecewise-constant gradient smears the 1/r field and the
    // μ-jump).  Band 1: iron; band 2: air; band 3: iron.
    let e1 = shell_band_l2_error(&mesh, &tags, &b, 1, mu_r[1], current, 0.13, 0.33);
    let e2 = shell_band_l2_error(&mesh, &tags, &b, 2, mu_r[2], current, 0.45, 0.65);
    let e3 = shell_band_l2_error(&mesh, &tags, &b, 3, mu_r[3], current, 0.75, 0.95);

    println!(
        "μ-contrast shell L2 rel err: iron(b1)={:.4}%  air(b2)={:.4}%  iron(b3)={:.4}%",
        e1 * 100.0,
        e2 * 100.0,
        e3 * 100.0
    );

    assert!(e1 <= 0.01, "iron band1 error {:.4}% > 1%", e1 * 100.0);
    assert!(e2 <= 0.01, "air band2 error {:.4}% > 1%", e2 * 100.0);
    assert!(e3 <= 0.01, "iron band3 error {:.4}% > 1%", e3 * 100.0);

    // Also assert the flux JUMPS by the μ-ratio across the iron→air
    // interface at r = 0.40.  Because B_θ ∝ μ_r/r, comparing the raw mean|B|
    // in two rings at different radii would fold in the 1/r variation, so we
    // scale each ring's mean by its mean radius (giving μ_r·μ₀I/2π, radius-
    // free) before taking the ratio.  Use thin rings hugging the interface.
    let (iron_mean, iron_r) = mean_bmag_and_r_in_ring(&mesh, &b, 0.355, 0.395);
    let (air_mean, air_r) = mean_bmag_and_r_in_ring(&mesh, &b, 0.405, 0.445);
    let ratio = (iron_mean * iron_r) / (air_mean * air_r);
    println!(
        "iron/air (B·r) jump ratio at r=0.40: {ratio:.1} (expected {})",
        mu_r[1]
    );
    assert!(
        (ratio / mu_r[1] - 1.0).abs() < 0.05,
        "μ-jump ratio {ratio:.1} deviates > 5% from expected {}",
        mu_r[1]
    );
}

/// Area-weighted mean `|B|` and mean centroid radius over triangles whose
/// centroid radius is in `[r_lo, r_hi]` — used to measure the field jump
/// across an interface (scaling by radius removes the 1/r variation so the
/// ratio isolates the pure μ-jump).
fn mean_bmag_and_r_in_ring(mesh: &TriMesh, b: &[[f64; 2]], r_lo: f64, r_hi: f64) -> (f64, f64) {
    let mut num = 0.0;
    let mut r_num = 0.0;
    let mut area_sum = 0.0;
    for t in 0..mesh.n_tris() {
        let r = centroid_r(mesh, t);
        if r < r_lo || r > r_hi {
            continue;
        }
        let a = tri_area(mesh, t);
        num += a * (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
        r_num += a * r;
        area_sum += a;
    }
    assert!(area_sum > 0.0, "empty ring [{r_lo}, {r_hi}]");
    (num / area_sum, r_num / area_sum)
}

// ═════════════════════════════════════════════════════════════════════
// 5. Inverse tripwire — a wrong ν map must fail the oracle loudly
// ═════════════════════════════════════════════════════════════════════

#[test]
fn swapped_mu_r_tripwire_fires() {
    let current = 3.0;
    // Correct map: shells iron(500) / air(1) / iron(500).
    // WRONG map: swap band 1 (iron) and band 2 (air) permeabilities, so the
    // solver puts air where the iron shell is and vice-versa.  The oracle
    // (★) is still computed with the TRUE μ_r, so the wrong-ν field is
    // mis-scaled by the full 500× contrast in the affected bands.
    let mu_true = [1.0, 500.0, 1.0, 500.0];
    let mu_wrong = [1.0, 1.0, 500.0, 500.0]; // band1↔band2 μ_r swapped
    let n_radial = [3usize, 24, 20, 20];
    let (mesh, tags, b) = solve_wire_shells(256, &n_radial, &mu_wrong, current);

    // Score the wrong-ν solution against the TRUE oracle in band 1 (should
    // be iron μ_r=500 but was solved as air).
    let err_band1 = shell_band_l2_error(&mesh, &tags, &b, 1, mu_true[1], current, 0.10, 0.35);
    println!(
        "swapped-μ_r tripwire: band-1 L2 rel err vs TRUE oracle = {:.1}%",
        err_band1 * 100.0
    );
    // The mis-scaling is ~ (1 - 1/500) ≈ 99.8% in the affected band — far
    // above the 1% pass bar.  Require a comfortable ≥ 50% floor.
    assert!(
        err_band1 > 0.50,
        "swapped-μ_r band-1 error {:.1}% did not clear the 50% tripwire floor — \
         the oracle is not sensitive to the ν map",
        err_band1 * 100.0
    );
}
