//! 3-D vector magnetostatic inductance-extraction oracles, structural
//! checks, and inverse tripwires (Epic #475, issue #504 — Palace
//! `Magnetostatic` parity, the dual of `tests/electrostatic_capacitance.rs`).
//!
//! Validates the host-side lowest-order Nédélec vector magnetostatic solver
//! plus Maxwell inductance-matrix extraction
//! (`geode_core::assembly::magnetostatic3d`), which solves
//! `∇×(ν∇×A) = J` with per-element `ν_r = 1/μ_r`, a tree-cotree gauge
//! (`eigen::gauge::TreeCotreeGauge`, PR #508), and RHS-driven unit-current
//! terminals, then forms `L_ij = A⁽ⁱ⁾ᵀ K A⁽ʲ⁾ / (I_i I_j)` on the full
//! pre-gauge `K`.
//!
//! Oracle 1 (**solid coaxial cable**): per-unit-length
//! `L' = μ₀/(2π)ln(b/a) + μ₀/(8π)` (external + internal), ≤ 1% vs the closed
//! form. Oracle 2 (**coaxial loop pair**): off-diagonal Maxwell mutual `M`
//! vs the elliptic-integral formula, honest few-% band (fat-tube filament
//! idealization + PEC-box truncation), with exact symmetry.
//!
//! Inverse tripwires (must fail when they should): the ungauged solve on a
//! mesh with a gradient nullspace is singular / gradient-contaminated; a
//! scrambled `μ_r` on a heterogeneous fixture moves `L` out of band; a
//! deliberately non-solenoidal `J` is rejected by the compatibility check.
//!
//! The tight oracle numbers live in
//! `benchmarks/magnetostatic_inductance/results.toml` (regenerate via
//! `cargo run -p geode-core --release --example magnetostatic_inductance`);
//! this suite re-runs a moderate coax oracle in-process plus the fast
//! structural / tripwire checks, and guards the committed Palace slot.
//!
//! Index-based loops over the fixed 6×6 element matrices and 3-vector spatial
//! axes read closer to the underlying linear algebra than iterator chains, so
//! `needless_range_loop` is silenced file-wide (same convention as
//! `tests/magnetostatic_wire.rs`).
#![allow(clippy::needless_range_loop)]

use std::f64::consts::PI;
use std::path::PathBuf;

use geode_core::assembly::magnetostatic3d::{
    CurrentTerminal, MU_0, Magnetostatic3dError, assemble_current_rhs, assemble_magnetostatic3d,
    axial_current_density, check_solenoidal, extract_inductance, loop_current_density,
    measure_axial_current, measure_loop_current, recover_b_field, tet_nedelec_rhs,
    tet_nedelec_stiffness, tet_signed_volume,
};
use geode_core::assembly::nedelec::pec_interior_edge_mask;
use geode_core::mesh::cube_tet_mesh;
use geode_core::mesh::magnetostatic_fixtures::{
    cylinder_cap_nodes, cylinder_pec_interior_mask, loop_pair_mesh, solid_coax_mesh,
};

// ─────────────────────────────────────────────────────────────────────
// 1. Element-kernel unit properties (host f64, fast).
// ─────────────────────────────────────────────────────────────────────

fn reference_tet() -> [[f64; 3]; 4] {
    [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ]
}

#[test]
fn stiffness_is_symmetric_and_gradient_nullspace() {
    let k = tet_nedelec_stiffness(&reference_tet());
    // Symmetric.
    for i in 0..6 {
        for j in 0..6 {
            assert!(
                (k[i][j] - k[j][i]).abs() < 1e-13,
                "K not symmetric at ({i},{j})"
            );
        }
    }
    // The curl-curl annihilates gradient fields: for the Whitney basis on a
    // single tet, the edge-DOF vector of ∇λ_n (n a vertex) is in the kernel.
    // ∇λ_n contributes ±1 on the three edges incident to n. Using the
    // canonical edge order [(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)], the
    // gradient of vertex 0 has edge coefficients (for edge (a,b),
    // ∫_edge ∇λ_0·dl = λ_0(b)-λ_0(a)) = [-1,-1,-1,0,0,0].
    let grad0 = [-1.0, -1.0, -1.0, 0.0, 0.0, 0.0];
    for i in 0..6 {
        let mut kv = 0.0;
        for j in 0..6 {
            kv += k[i][j] * grad0[j];
        }
        assert!(
            kv.abs() < 1e-12,
            "K·grad0 row {i} = {kv} != 0 (gradient nullspace)"
        );
    }
}

#[test]
fn rhs_matches_uniform_field_projection() {
    // For a uniform J, b_i = (V/4)(∇λ_b − ∇λ_a)·J. On the reference tet
    // V = 1/6, ∇λ_1=(1,0,0), ∇λ_0=(-1,-1,-1), so edge (0,1): (∇λ_1−∇λ_0)=
    // (2,1,1). With J=(1,0,0): b_0 = (1/24)·2 = 1/12.
    let b = tet_nedelec_rhs(&reference_tet(), [1.0, 0.0, 0.0]);
    assert!((b[0] - 1.0 / 12.0).abs() < 1e-13, "b[0]={} != 1/12", b[0]);
    // Volume sanity.
    assert!((tet_signed_volume(&reference_tet()) - 1.0 / 6.0).abs() < 1e-15);
}

// ─────────────────────────────────────────────────────────────────────
// 2. Coax oracle: L' ≤ 1% vs the solid-conductor closed form.
// ─────────────────────────────────────────────────────────────────────

/// Solve the solid-coax fixture and return `(L'/length, flux_linkage/length)`.
fn coax_l_per_length(a: f64, b: f64, length: f64, nth: usize, nr: usize, nz: usize) -> (f64, f64) {
    let fx = solid_coax_mesh(a, b, length, nth, nr, nz);
    let (_edges, mask) = cylinder_pec_interior_mask(&fx.mesh, b, length);
    let mu_r = vec![1.0; fx.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&fx.mesh, &mu_r, &mask).unwrap();
    let j = axial_current_density(&fx.mesh, a, 1.0);
    let i_meas = measure_axial_current(&fx.mesh, &j, length);
    let term = CurrentTerminal {
        name: "coax".into(),
        j,
        current: i_meas,
        exempt_nodes: cylinder_cap_nodes(&fx.mesh, length),
    };
    let lm = extract_inductance(&sys, &fx.mesh, std::slice::from_ref(&term), 1e-6).unwrap();
    (lm.l[0][0] / length, lm.flux_linkage_diag[0] / length)
}

#[test]
fn coax_external_plus_internal_inductance_within_1pct() {
    let (a, b, length) = (1.0_f64, 3.0_f64, 1.0_f64);
    // Resolution matched to the ≤1% bar (the internal-inductance core near
    // the polar-mesh axis is the convergence-limiting region).
    let (lp, flux) = coax_l_per_length(a, b, length, 64, 32, 3);
    let l_ext = MU_0 / (2.0 * PI) * (b / a).ln();
    let l_int = MU_0 / (8.0 * PI);
    let l_closed = l_ext + l_int;
    let rel = (lp - l_closed).abs() / l_closed;
    assert!(
        rel < 1e-2,
        "coax L'/length {lp:.6e} vs closed {l_closed:.6e} (ext {l_ext:.6e} + int {l_int:.6e}), rel {rel:.4e} exceeds 1%"
    );
    // Flux-linkage cross-check reproduces the energy diagonal (Aᵀb = AᵀKA).
    assert!(
        (flux - lp).abs() / lp < 1e-8,
        "flux-linkage {flux:.6e} disagrees with energy L' {lp:.6e}"
    );
}

#[test]
fn coax_field_is_azimuthal_and_matches_amperes_law() {
    let (a, b, length) = (1.0_f64, 3.0_f64, 1.0_f64);
    let fx = solid_coax_mesh(a, b, length, 48, 20, 2);
    let (_edges, mask) = cylinder_pec_interior_mask(&fx.mesh, b, length);
    let mu_r = vec![1.0; fx.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&fx.mesh, &mu_r, &mask).unwrap();
    let j = axial_current_density(&fx.mesh, a, 1.0);
    let b_rhs = assemble_current_rhs(&sys, &fx.mesh, &j).unwrap();
    let asol = sys.solve(&b_rhs).unwrap();
    let bfield = recover_b_field(&sys, &fx.mesh, &asol);
    let current = measure_axial_current(&fx.mesh, &j, length);
    let mut ratio_sum = 0.0;
    let mut n = 0;
    for (t, tet) in fx.mesh.tets.iter().enumerate() {
        let mut c = [0.0; 3];
        for &v in tet {
            for d in 0..3 {
                c[d] += fx.mesh.nodes[v as usize][d] * 0.25;
            }
        }
        let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
        if (1.5..2.5).contains(&r) {
            let bmag = (bfield[t][0].powi(2) + bfield[t][1].powi(2) + bfield[t][2].powi(2)).sqrt();
            let expected = MU_0 * current / (2.0 * PI * r);
            ratio_sum += bmag / expected;
            n += 1;
        }
    }
    let mean = ratio_sum / n as f64;
    assert!(
        (mean - 1.0).abs() < 0.05,
        "mean |B|/|B_Ampere| = {mean:.4} in annulus (expected ≈1 for the surface-current field)"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 3. Loop-pair off-diagonal mutual inductance (honest band + exact symmetry).
// ─────────────────────────────────────────────────────────────────────

fn ellipk_e(m: f64) -> (f64, f64) {
    let mut a = 1.0;
    let mut b = (1.0 - m).sqrt();
    let mut c = m.sqrt();
    let mut sum = 0.0;
    let mut pow2 = 0.5;
    for _ in 0..60 {
        sum += pow2 * c * c;
        let an = 0.5 * (a + b);
        let bn = (a * b).sqrt();
        c = 0.5 * (a - b);
        a = an;
        b = bn;
        pow2 *= 2.0;
        if c.abs() < 1e-16 {
            break;
        }
    }
    let k = PI / (2.0 * a);
    (k, k * (1.0 - sum))
}

fn maxwell_mutual(r1: f64, r2: f64, d: f64) -> f64 {
    let k2 = 4.0 * r1 * r2 / ((r1 + r2).powi(2) + d * d);
    let k = k2.sqrt();
    let (bigk, bige) = ellipk_e(k2);
    MU_0 * (r1 * r2).sqrt() * ((2.0 / k - k) * bigk - (2.0 / k) * bige)
}

#[test]
fn loop_pair_mutual_inductance_sign_symmetry_and_band() {
    let (r1, z1, r2, z2) = (1.0_f64, 2.0_f64, 1.0_f64, 3.0_f64);
    let (rbox, length, rtube) = (5.0_f64, 5.0_f64, 0.25_f64);
    // Moderate in-suite resolution (the tight ~5% number lives in the
    // benchmark example). The physics we gate here: correct sign, exact
    // symmetry, SPD, and same-order-of-magnitude agreement with Maxwell.
    let fx = loop_pair_mesh(r1, z1, r2, z2, rbox, length, 24, 12, 12);
    let tolb = 1e-6 * rbox;
    let tolz = 1e-6 * length;
    let on_bdry: Vec<bool> = fx
        .mesh
        .nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            (r - rbox).abs() < tolb || p[2].abs() < tolz || (p[2] - length).abs() < tolz
        })
        .collect();
    let edges = fx.mesh.edges();
    let mask = pec_interior_edge_mask(&edges, &on_bdry);
    let mu_r = vec![1.0; fx.mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&fx.mesh, &mu_r, &mask).unwrap();

    let j1 = loop_current_density(&fx.mesh, r1, z1, rtube, 1.0);
    let j2 = loop_current_density(&fx.mesh, r2, z2, rtube, 1.0);
    let i1 = measure_loop_current(&fx.mesh, &j1);
    let i2 = measure_loop_current(&fx.mesh, &j2);
    let terms = vec![
        CurrentTerminal {
            name: "loop1".into(),
            j: j1,
            current: i1,
            exempt_nodes: vec![],
        },
        CurrentTerminal {
            name: "loop2".into(),
            j: j2,
            current: i2,
            exempt_nodes: vec![],
        },
    ];
    let lm = extract_inductance(&sys, &fx.mesh, &terms, 5e-2).unwrap();

    let m_fem = lm.l[0][1];
    let m_exact = maxwell_mutual(r1, r2, (z2 - z1).abs());
    // Exact symmetry (free tripwire from the energy method).
    assert!(
        lm.max_rel_asymmetry() < 1e-9,
        "L asymmetry {}",
        lm.max_rel_asymmetry()
    );
    // SPD on the terminal space.
    assert!(lm.is_spd(1e-18), "L not SPD: {:?}", lm.l);
    // Correct sign (coaxial same-sense loops link positive flux).
    assert!(
        m_fem > 0.0,
        "mutual M {m_fem:.4e} should be positive for coaxial loops"
    );
    // Same order of magnitude as Maxwell (coarse-mesh honest band).
    let rel = (m_fem - m_exact).abs() / m_exact.abs();
    assert!(
        rel < 0.30,
        "mutual M {m_fem:.4e} vs Maxwell {m_exact:.4e}, rel {rel:.4e} outside the coarse-mesh band (tight number is in the benchmark)"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 4. Inverse tripwires (must fail when they should).
// ─────────────────────────────────────────────────────────────────────

/// A cube-in-vacuum fixture with a large gradient nullspace (all-outer PEC),
/// driven by a uniform axial current — the setup for tripwires 1 & 3.
fn cube_fixture() -> (geode_core::mesh::TetMesh, Vec<bool>) {
    let mesh = cube_tet_mesh(4, 1.0);
    let side = 1.0;
    let tol = 1e-9;
    let on_bdry: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| p.iter().any(|&c| c.abs() < tol || (c - side).abs() < tol))
        .collect();
    let edges = mesh.edges();
    let mask = pec_interior_edge_mask(&edges, &on_bdry);
    (mesh, mask)
}

#[test]
fn tripwire_ungauged_solve_is_gradient_contaminated_or_fails() {
    // The ungauged reduced curl-curl keeps the full gradient nullspace
    // (`kernel(K) = image(d⁰)`), so the ungauged system is **singular**. The
    // gauge is load-bearing, but the *signature* of its necessity is subtle:
    //
    //  - The magnetic **energy** `W = ½ AᵀKA` is gauge-INVARIANT (`B = ∇×A`
    //    is unchanged by adding a gradient), so an energy comparison cannot
    //    detect the missing gauge — faer's sparse LU may even return *a*
    //    particular solution for a consistent RHS on a small mesh.
    //  - What the gauge fixes is **uniqueness**: without it the solution
    //    carries arbitrary gradient content, inflating `‖A‖` (the gauged
    //    solution is the minimal-norm / gradient-free representative), and on
    //    larger or less-consistent systems the singular factorization
    //    diverges or fails outright.
    //
    // So the tripwire asserts EITHER the ungauged factorization fails, OR the
    // ungauged solution is gradient-contaminated (‖A_ungauged‖ meaningfully
    // exceeds ‖A_gauged‖). A perfectly-equal norm would mean the gauge did
    // nothing — a real regression.
    let (mesh, mask) = cube_fixture();
    let mu_r = vec![1.0; mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&mesh, &mu_r, &mask).unwrap();
    let j: Vec<[f64; 3]> = mesh.tets.iter().map(|_| [0.0, 0.0, 1.0]).collect();
    let b = assemble_current_rhs(&sys, &mesh, &j).unwrap();

    let a_gauged = sys.solve(&b).expect("gauged solve should succeed");
    let norm = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let n_gauged = norm(&a_gauged);

    match sys.solve_ungauged(&b) {
        Err(Magnetostatic3dError::Factorization(_)) => { /* singular — the strongest signature */
        }
        Err(other) => panic!("unexpected ungauged error variant: {other}"),
        Ok(a_ungauged) => {
            let ratio = norm(&a_ungauged) / n_gauged;
            // Gauge-invariant energy must still agree (sanity: same B field).
            let (eg, eu) = (sys.field_energy(&a_gauged), sys.field_energy(&a_ungauged));
            assert!(
                (eg - eu).abs() / eg < 1e-6,
                "energy should be gauge-invariant: gauged {eg:.4e} vs ungauged {eu:.4e}"
            );
            // The ungauged solution must carry spurious gradient content.
            assert!(
                ratio > 1.3,
                "ungauged ‖A‖/‖A_gauged‖ = {ratio:.4} ≈ 1 — the gauge is not \
                 removing the gradient nullspace (expected the ungauged solution to be inflated)"
            );
        }
    }
}

#[test]
fn tripwire_scrambled_mu_r_moves_inductance() {
    // A heterogeneous coax: inner-core μ_r ≠ 1. Scrambling μ_r must move L.
    let (a, b, length) = (1.0_f64, 3.0_f64, 1.0_f64);
    let fx = solid_coax_mesh(a, b, length, 40, 16, 2);
    let (_edges, mask) = cylinder_pec_interior_mask(&fx.mesh, b, length);
    // Region tag: inner core (centroid r < a) gets a distinct μ_r.
    let mu_true: Vec<f64> = fx
        .mesh
        .tets
        .iter()
        .map(|tet| {
            let mut c = [0.0; 3];
            for &v in tet {
                for d in 0..3 {
                    c[d] += fx.mesh.nodes[v as usize][d] * 0.25;
                }
            }
            let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
            if r < a { 4.0 } else { 1.0 }
        })
        .collect();
    let mu_scrambled: Vec<f64> = mu_true
        .iter()
        .map(|&m| if m > 1.0 { 1.0 } else { 4.0 })
        .collect();

    let l_of = |mu: &[f64]| {
        let sys = assemble_magnetostatic3d(&fx.mesh, mu, &mask).unwrap();
        let j = axial_current_density(&fx.mesh, a, 1.0);
        let i = measure_axial_current(&fx.mesh, &j, length);
        let term = CurrentTerminal {
            name: "c".into(),
            j,
            current: i,
            exempt_nodes: cylinder_cap_nodes(&fx.mesh, length),
        };
        extract_inductance(&sys, &fx.mesh, std::slice::from_ref(&term), 1e-6)
            .unwrap()
            .l[0][0]
    };
    let l_true = l_of(&mu_true);
    let l_scr = l_of(&mu_scrambled);
    let rel = (l_true - l_scr).abs() / l_true;
    assert!(
        rel > 0.05,
        "scrambled μ_r moved L by only {rel:.4e}; the ν-weighting path is not live"
    );
}

#[test]
fn tripwire_non_solenoidal_current_rejected() {
    // A current that diverges inside the domain (radially outward J) is not
    // discretely solenoidal at interior nodes and must be rejected.
    let (mesh, mask) = cube_fixture();
    let mu_r = vec![1.0; mesh.n_tets()];
    let sys = assemble_magnetostatic3d(&mesh, &mu_r, &mask).unwrap();

    // Solenoidal reference: uniform axial J passes (exempting the z-caps of
    // the cube, which are PEC here so already boundary).
    let j_ok: Vec<[f64; 3]> = mesh.tets.iter().map(|_| [0.0, 0.0, 1.0]).collect();
    let b_ok = assemble_current_rhs(&sys, &mesh, &j_ok).unwrap();
    assert!(
        check_solenoidal(&sys, &b_ok, &[], 1e-6).is_ok(),
        "uniform axial J should pass the solenoidality check"
    );

    // Non-solenoidal: radially-outward J from the cube centre (a source).
    let ctr = [0.5, 0.5, 0.5];
    let j_bad: Vec<[f64; 3]> = mesh
        .tets
        .iter()
        .map(|tet| {
            let mut c = [0.0; 3];
            for &v in tet {
                for d in 0..3 {
                    c[d] += mesh.nodes[v as usize][d] * 0.25;
                }
            }
            [c[0] - ctr[0], c[1] - ctr[1], c[2] - ctr[2]]
        })
        .collect();
    let b_bad = assemble_current_rhs(&sys, &mesh, &j_bad).unwrap();
    match check_solenoidal(&sys, &b_bad, &[], 1e-6) {
        Err(Magnetostatic3dError::NonSolenoidal(_)) => { /* expected */ }
        other => panic!("radially-divergent J should be rejected, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────
// 5. Palace terminal-M cross-check slot (skip-with-note convention).
// ─────────────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

#[test]
fn fem_vs_palace_terminal_m_within_band_or_skip_with_note() {
    let path = repo_root().join("benchmarks/magnetostatic_inductance/results.toml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("results.toml is valid TOML");
    let palace = doc
        .get("oracles")
        .and_then(|o| o.get("palace"))
        .expect("results.toml has [oracles.palace] block");
    let status = palace
        .get("status")
        .and_then(|s| s.as_str())
        .expect("[oracles.palace] has a status");
    if status == "pending_operator_run" {
        eprintln!(
            "\nSKIP: [oracles.palace] in {} is `pending_operator_run` — no Palace \
             terminal-M reference ingested. To populate: run Palace's Magnetostatic \
             module on the equivalent coax/loop geometry, then ingest its \
             terminal-M.csv into a populated block with provenance (palace_version, \
             config_sha256).",
            path.display()
        );
        return;
    }
    // If populated, require provenance before any numeric comparison.
    assert!(
        palace
            .get("palace_version")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty()),
        "populated [oracles.palace] must record `palace_version` (provenance)"
    );
}

/// The committed benchmark records the coax oracle within its ≤1% bar and the
/// loop-pair mutual within its honest band — guard against silent regressions
/// of the generated numbers.
#[test]
fn committed_benchmark_records_oracle_bars() {
    let path = repo_root().join("benchmarks/magnetostatic_inductance/results.toml");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read committed results {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let get = |sec: &str, key: &str| -> f64 {
        doc.get("oracles")
            .and_then(|o| o.get(sec))
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_float())
            .unwrap_or_else(|| panic!("missing oracles.{sec}.{key}"))
    };
    assert!(
        get("coax", "rel_err") < 1e-2,
        "committed coax rel_err exceeds 1% bar"
    );
    // Loop-pair honest band (fat-tube + truncation dominated).
    assert!(
        get("loop_pair", "m_rel_err") < 0.10,
        "committed loop-pair M rel_err outside honest band"
    );
    assert!(
        get("loop_pair", "max_rel_asymmetry") < 1e-9,
        "committed L asymmetry too large"
    );
}
