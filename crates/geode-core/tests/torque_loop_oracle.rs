//! Torque-extractor acceptance benchmark (Epic #448, Phase 3a).
//!
//! Validates the two torque estimators — [`maxwell_stress_torque`] (Maxwell
//! stress-tensor air-gap line integral) and [`arkkio_torque`] (volume-averaged
//! Arkkio variant) — against the closed-form **torque-on-a-current-loop**
//! oracle, *before* they are trusted on a real machine solve.
//!
//! A current loop of moment (per axial length) `m = I d` in a uniform
//! external field `B` feels a torque `T = m × B`, `|T| = m B sinφ`, where φ
//! is the angle between the moment vector and `B`. In the 2-D per-unit-length
//! reduction the loop is a pair of antiparallel axial line currents `±I`
//! separated by `d`.
//!
//! Four-part plan (matching the issue):
//! 1. **Synthetic-field unit test** — both estimators integrate a hand-coded
//!    analytic total field to ≤0.1 % of the exact `m × B` (isolates the
//!    integrator from FE error; no solve).
//! 2. **Loop oracle, line integral** — FE-solved loop field + analytic
//!    uniform field, swept φ ∈ [0, π]: line-integral torque ≤2 %.
//! 3. **Loop oracle, Arkkio** — same sweep, tighter ≤1 % (averaging).
//! 4. **Inverse tripwire** — a wrong extraction radius (contour outside the
//!    enclosing gap, so it no longer encloses the loop currents) or a
//!    flipped current sign drives the error above a floor > the pass bar.
//!
//! ## Design decisions
//!
//! **Uniform imposed field by superposition (not a solver change).** The
//! merged `assemble_magnetostatic` only supports the `A_z = 0` Dirichlet BC.
//! Rather than extend it with nonzero-Dirichlet support, the uniform external
//! field is added *analytically* to the FE-recovered loop field before torque
//! extraction. This is exact for linear materials (ν = 1 uniform air here) —
//! the loop field and the uniform field superpose — and the torque integrand
//! needs the *total* field. Zero solver changes.
//!
//! **Finite-radius wires, not delta filaments.** A line current is a delta
//! source the piecewise-constant `j_z` assembler cannot represent. Each
//! current is instead a small uniform-`J_z` disk (`∫ J_z dA = ±I`). By
//! Ampère's law the field *outside* a uniform-current disk is identical to
//! the ideal line current, so on the extraction contour (which encloses both
//! wire disks) the FE field matches the ideal loop field and the extracted
//! torque matches `m × B`. The antiparallel pair carries *zero* net current,
//! so its far field decays like a dipole (~1/r²) and the finite-domain
//! `A_z = 0` truncation error at the outer boundary is small.

use geode_core::analytic::slotless_pm::MU_0;
use geode_core::analytic::waveguide::{
    RadialGrading, TriMesh, disk_boundary_nodes, disk_tri_mesh_bands,
};
use geode_core::assembly::magnetostatic::{assemble_magnetostatic, recover_b_field};
use geode_core::assembly::torque::{
    arkkio_torque, cartesian_to_polar_b, line_currents_plus_uniform_b, maxwell_stress_torque,
    maxwell_stress_torque_from_samples,
};

// Band layout of the loop-oracle disk mesh, radii = [0, r1, r2, r3, rout]:
//   band 0 = [0, r1)     inner region holding the two current wires
//   band 1 = [r1, r2)    buffer air
//   band 2 = [r2, r3)    EXTRACTION annulus (the "air gap" for Arkkio)
//   band 3 = [r3, rout)  outer air to the Dirichlet boundary
const TAG_GAP: i32 = 2;

/// Loop-oracle geometry and physical constants.
struct Loop {
    /// Wire-pair half-separation: `+I` and `−I` sit at radius `d/2` from
    /// centre, so the separation is `d`.
    d: f64,
    /// Current magnitude (A).
    current: f64,
    /// Wire disk radius (finite, so `j_z` can represent it).
    r_wire: f64,
    /// Uniform external field magnitude (T), directed along `+x̂`.
    b_ext: f64,
    /// Mesh band radii.
    radii: [f64; 5],
}

impl Loop {
    fn nominal() -> Self {
        Loop {
            d: 0.10,
            current: 100.0,
            r_wire: 0.02,
            b_ext: 0.5,
            radii: [0.0, 0.20, 0.35, 0.50, 1.20],
        }
    }
    /// Moment per axial length, `m = I d`.
    fn moment(&self) -> f64 {
        self.current * self.d
    }
    /// Mid-extraction-annulus radius (the line-integral contour).
    fn r_gap(&self) -> f64 {
        0.5 * (self.radii[2] + self.radii[3])
    }
    fn rout(&self) -> f64 {
        self.radii[4]
    }
    /// Line-current sources `(x, y, I)` for moment angle φ (from `+x̂`).
    ///
    /// The moment `m` points at angle φ; the current-separation vector is
    /// therefore at `φ − 90°`, so `+I` sits at `(d/2)(sinφ, −cosφ)` and
    /// `−I` diametrically opposite. Then `m` at angle φ crossed into
    /// `B = B x̂` gives `|T| = m B sinφ`.
    fn sources(&self, phi: f64) -> [(f64, f64, f64); 2] {
        let h = 0.5 * self.d;
        let (sp, cp) = (phi.sin(), phi.cos());
        [
            (h * sp, -h * cp, self.current),
            (-h * sp, h * cp, -self.current),
        ]
    }
}

/// Build the four-band loop-oracle mesh (μ_r = 1 everywhere → ν = 1).
fn build_mesh(l: &Loop, n_ang: usize, n_rad: [usize; 4]) -> (TriMesh, Vec<i32>) {
    disk_tri_mesh_bands(&l.radii, n_ang, &n_rad, &[RadialGrading::Uniform; 4])
}

/// Solve the FE loop field for moment angle φ and add the uniform external
/// field analytically. Returns `(mesh, tags, b_total)` with `b_total` the
/// per-triangle total flux density `[B_x, B_y]`.
fn solve_loop_total_field(
    l: &Loop,
    phi: f64,
    n_ang: usize,
    n_rad: [usize; 4],
) -> (TriMesh, Vec<i32>, Vec<[f64; 2]>) {
    let (mesh, tags) = build_mesh(l, n_ang, n_rad);
    let n_tris = mesh.n_tris();
    let nu = vec![1.0; n_tris];

    // Assign the two finite-radius wires: uniform J_z over the triangles
    // whose centroid lies within r_wire of each current centre, scaled so
    // ∫ J_z dA = ±I exactly (using the actual summed area of the tagged
    // triangles, so the enclosed current is exact regardless of meshing).
    let src = l.sources(phi);
    let mut in_wire: Vec<Option<usize>> = vec![None; n_tris];
    let mut area_sum = [0.0_f64; 2];
    let centroid = |tri: &[u32; 3]| {
        let c0 = mesh.nodes[tri[0] as usize];
        let c1 = mesh.nodes[tri[1] as usize];
        let c2 = mesh.nodes[tri[2] as usize];
        let cx = (c0[0] + c1[0] + c2[0]) / 3.0;
        let cy = (c0[1] + c1[1] + c2[1]) / 3.0;
        let area =
            0.5 * ((c1[0] - c0[0]) * (c2[1] - c0[1]) - (c1[1] - c0[1]) * (c2[0] - c0[0])).abs();
        (cx, cy, area)
    };
    for (t, tri) in mesh.tris.iter().enumerate() {
        let (cx, cy, area) = centroid(tri);
        for (w, &(x0, y0, _)) in src.iter().enumerate() {
            let dr = ((cx - x0).powi(2) + (cy - y0).powi(2)).sqrt();
            if dr <= l.r_wire {
                in_wire[t] = Some(w);
                area_sum[w] += area;
                break;
            }
        }
    }
    assert!(
        area_sum[0] > 0.0 && area_sum[1] > 0.0,
        "wire regions empty — mesh too coarse for r_wire={}",
        l.r_wire
    );
    // The scalar magnetostatic assembler solves −∇·(ν∇A_z) = J_z with the
    // μ₀ folded into the SOURCE (convention shared with the straight-wire
    // oracle: `density = μ₀ · I / area`), so the recovered A_z/B come out in
    // physical SI (Tesla). Scale each wire's uniform density accordingly so
    // the enclosed current is exactly ±I.
    let mut j_z = vec![0.0; n_tris];
    for (t, w) in in_wire.iter().enumerate() {
        if let Some(w) = w {
            let (_, _, cur) = src[*w];
            j_z[t] = MU_0 * cur / area_sum[*w];
        }
    }

    let bc = disk_boundary_nodes(&mesh, l.rout());
    let sys = assemble_magnetostatic(&mesh, &nu, &j_z, &bc).expect("assemble loop");
    let a_z = sys.solve().expect("loop solve");
    let b_loop = recover_b_field(&mesh, &a_z);

    // Superpose the uniform external field B = (b_ext, 0) analytically.
    let b_total: Vec<[f64; 2]> = b_loop.iter().map(|&[bx, by]| [bx + l.b_ext, by]).collect();
    (mesh, tags, b_total)
}

// ─────────────────────────────────────────────────────────────────────
// 1. Synthetic-field unit test (≤0.1 %, no FE solve)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn synthetic_field_estimators_match_oracle() {
    // Hand-build the analytic total field (ideal line currents + uniform B)
    // sampled per triangle on a fine mesh, and feed it straight to both
    // estimators — bypassing the FE solve entirely. This isolates the
    // integrators from any finite-element error: they must reproduce the
    // exact m × B to numerical-quadrature tolerance.
    let l = Loop::nominal();
    let m = l.moment();
    let n_ang = 720;
    let n_rad = [24, 8, 6, 20];
    let (mesh, tags) = build_mesh(&l, n_ang, n_rad);

    let mut worst_line = 0.0_f64;
    let mut worst_arkkio = 0.0_f64;
    for k in 0..=8 {
        let phi = std::f64::consts::PI * k as f64 / 8.0;
        let src = l.sources(phi);

        // LINE INTEGRAL: sample the analytic total field at the EXACT contour
        // points (not via triangle centroids), so the only error is the
        // θ-quadrature + prefactor — the true "integrator isolated from FE
        // error" test the issue calls for.
        let n_contour = 360;
        let r_gap = l.r_gap();
        let samples: Vec<(f64, f64)> = (0..n_contour)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / n_contour as f64;
                let (px, py) = (r_gap * theta.cos(), r_gap * theta.sin());
                let bxy = line_currents_plus_uniform_b(&src, [l.b_ext, 0.0], px, py);
                cartesian_to_polar_b(bxy, theta)
            })
            .collect();
        let t_line = maxwell_stress_torque_from_samples(&samples, r_gap, 1.0);

        // ARKKIO: hand-coded analytic field array per triangle centroid on a
        // fine mesh, fed to the volume-averaging estimator.
        let b: Vec<[f64; 2]> = mesh
            .tris
            .iter()
            .map(|tri| {
                let c0 = mesh.nodes[tri[0] as usize];
                let c1 = mesh.nodes[tri[1] as usize];
                let c2 = mesh.nodes[tri[2] as usize];
                let cx = (c0[0] + c1[0] + c2[0]) / 3.0;
                let cy = (c0[1] + c1[1] + c2[1]) / 3.0;
                line_currents_plus_uniform_b(&src, [l.b_ext, 0.0], cx, cy)
            })
            .collect();

        let t_exact = m * l.b_ext * phi.sin();
        let t_ark = arkkio_torque(&mesh, &tags, &b, TAG_GAP, l.radii[2], l.radii[3], 1.0);

        let denom = m * l.b_ext; // peak |T|; robust near φ = 0, π
        let e_line = (t_line - t_exact).abs() / denom;
        let e_ark = (t_ark - t_exact).abs() / denom;
        println!(
            "synthetic φ={:.3}π  T_exact={:+.4e}  line err={:.4}%  arkkio err={:.4}%",
            k as f64 / 8.0,
            t_exact,
            e_line * 100.0,
            e_ark * 100.0
        );
        worst_line = worst_line.max(e_line);
        worst_arkkio = worst_arkkio.max(e_ark);
    }
    println!(
        "synthetic worst: line={:.4}%  arkkio={:.4}%",
        worst_line * 100.0,
        worst_arkkio * 100.0
    );
    assert!(
        worst_line <= 1e-3,
        "synthetic line-integral worst error {:.4}% exceeds 0.1% quadrature bar",
        worst_line * 100.0
    );
    assert!(
        worst_arkkio <= 1e-3,
        "synthetic Arkkio worst error {:.4}% exceeds 0.1% quadrature bar",
        worst_arkkio * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// 2 & 3. Loop oracle: line integral (≤2 %) and Arkkio (≤1 %)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn loop_torque_line_and_arkkio_vs_m_cross_b() {
    let l = Loop::nominal();
    let m = l.moment();
    let denom = m * l.b_ext; // peak torque, for robust rel error near φ=0,π
    let n_ang = 600;
    let n_rad = [24, 8, 7, 20];
    // Sample the contour finer than the mesh angular resolution: matching
    // n_contour to n_ang aliases the sample ring against the radial mesh
    // lines and makes the single-contour line integral jitter (a known
    // coarse-mesh sensitivity of the pointwise Maxwell-stress estimator).
    // Oversampling in θ decouples the quadrature from the mesh and the line
    // integral converges smoothly.
    let n_contour = 720;

    let mut worst_line = 0.0_f64;
    let mut worst_arkkio = 0.0_f64;
    println!(
        "--- loop torque T(φ) vs m B sinφ  (m={m}, B={}) ---",
        l.b_ext
    );
    for k in 0..=8 {
        let phi = std::f64::consts::PI * k as f64 / 8.0;
        let (mesh, tags, b) = solve_loop_total_field(&l, phi, n_ang, n_rad);
        let t_exact = m * l.b_ext * phi.sin();
        let t_line = maxwell_stress_torque(&mesh, &b, l.r_gap(), 1.0, n_contour);
        let t_ark = arkkio_torque(&mesh, &tags, &b, TAG_GAP, l.radii[2], l.radii[3], 1.0);
        let e_line = (t_line - t_exact).abs() / denom;
        let e_ark = (t_ark - t_exact).abs() / denom;
        println!(
            "φ={:.3}π  T_exact={:+.4e}  T_line={:+.4e} ({:.3}%)  T_ark={:+.4e} ({:.3}%)",
            k as f64 / 8.0,
            t_exact,
            t_line,
            e_line * 100.0,
            t_ark,
            e_ark * 100.0
        );
        worst_line = worst_line.max(e_line);
        worst_arkkio = worst_arkkio.max(e_ark);
    }
    println!(
        "loop worst: line={:.3}%  arkkio={:.3}%",
        worst_line * 100.0,
        worst_arkkio * 100.0
    );
    assert!(
        worst_line <= 0.02,
        "loop line-integral worst error {:.3}% exceeds the 2% bar",
        worst_line * 100.0
    );
    assert!(
        worst_arkkio <= 0.01,
        "loop Arkkio worst error {:.3}% exceeds the 1% bar",
        worst_arkkio * 100.0
    );
}

// ─────────────────────────────────────────────────────────────────────
// 4. Inverse tripwires (error above a floor > the pass bar)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn wrong_current_sign_tripwire_fires() {
    // Flip BOTH currents' sign: the loop field (and hence its cross term
    // with B) flips, so the extracted torque flips sign. At the peak φ=π/2
    // that is a ~200% relative error — far above the 2%/1% pass bars.
    let mut l = Loop::nominal();
    l.current = -l.current; // sign flip
    let m = l.current.abs() * l.d;
    let denom = m * l.b_ext;
    let n_ang = 360;
    let n_rad = [18, 6, 5, 16];
    let phi = std::f64::consts::FRAC_PI_2;
    let (mesh, tags, b) = solve_loop_total_field(&l, phi, n_ang, n_rad);
    // Oracle for the CORRECT (positive) current at this φ.
    let t_exact = m * l.b_ext * phi.sin();
    let t_line = maxwell_stress_torque(&mesh, &b, l.r_gap(), 1.0, 360);
    let t_ark = arkkio_torque(&mesh, &tags, &b, TAG_GAP, l.radii[2], l.radii[3], 1.0);
    let e_line = (t_line - t_exact).abs() / denom;
    let e_ark = (t_ark - t_exact).abs() / denom;
    println!(
        "wrong-sign tripwire: line err={:.1}%  arkkio err={:.1}%",
        e_line * 100.0,
        e_ark * 100.0
    );
    assert!(
        e_line > 0.5 && e_ark > 0.5,
        "wrong-sign error (line {:.1}%, arkkio {:.1}%) did not exceed the 50% floor — \
         tripwire broken",
        e_line * 100.0,
        e_ark * 100.0
    );
}

#[test]
fn wrong_extraction_radius_tripwire_fires() {
    // Move the line-integral contour OUTSIDE the meshed extraction annulus
    // is impossible (it would leave the mesh); instead move it INSIDE the
    // wire pair (r_gap < d/2) so the contour no longer encloses both
    // currents. Ampère/stress-tensor then reports a wrong (much smaller)
    // enclosed torque — a gross error above the 2% pass bar. This proves a
    // passing extractor test is not trivially satisfiable by any r_gap.
    let l = Loop::nominal();
    let m = l.moment();
    let denom = m * l.b_ext;
    let n_ang = 360;
    let n_rad = [18, 6, 5, 16];
    let phi = std::f64::consts::FRAC_PI_2;
    let (mesh, _tags, b) = solve_loop_total_field(&l, phi, n_ang, n_rad);
    let t_exact = m * l.b_ext * phi.sin();
    // Contour at r = d/4 < d/2 = wire radius-of-placement: encloses no net
    // current arrangement resembling the full loop.
    let r_bad = 0.25 * l.d;
    let t_bad = maxwell_stress_torque(&mesh, &b, r_bad, 1.0, 360);
    let e_bad = (t_bad - t_exact).abs() / denom;
    println!(
        "wrong-radius tripwire: r_bad={r_bad} T_bad={:+.4e} T_exact={:+.4e} err={:.1}%",
        t_bad,
        t_exact,
        e_bad * 100.0
    );
    assert!(
        e_bad > 0.05,
        "wrong-radius error {:.1}% did not exceed the 5% floor (> the 2% pass bar) — \
         tripwire broken",
        e_bad * 100.0
    );
}
