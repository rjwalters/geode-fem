//! `TabulatedBhLaw` on the **measured M27 B-H table** of the motor
//! reference (motor paper, issue #1160).
//!
//! The 47 points (0–2.30 T) are read from
//! `benchmarks/motor_reference/reference_data.json` (`material.B_T`,
//! `material.H_A_per_m`). The tests check:
//!
//! 1. construction succeeds and the interpolant is exact at every knot;
//! 2. `dH/dB > 0`, `ν > 0` and `ν + 2 s ν'(s) > 0` (s = |B|²) on a dense
//!    grid from 0 to 3 T, as `conventions.json`
//!    (`positive_tangent_condition`) requires;
//! 3. `ν'(s) < 0` somewhere at low field. M27 has `H/B` falling from 341
//!    to 256 m/H between 0.05 and 0.10 T, so a monotone `ν` is **not**
//!    required and the law must not impose one;
//! 4. central-FD checks of `dnu_db2` against `nu` and of the energy
//!    density against `nu/2`, through every segment and the tail;
//! 5. one saturated-iron Ampère coax solve with M27 iron, against the exact
//!    `|B|(r) = B(I/(2πr))` oracle, as in `magnetostatic_nonlinear.rs`.
//!
//! The measured difference between this law and the reference code's
//! MATLAB `pchip` is computed by
//! `benchmarks/motor_nonlinear_m1/pchip_compare.py` from the dense dump of
//! `examples/m27_law_dump.rs`; see that directory's README.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::TAU;

use geode_core::analytic::waveguide::{TriMesh, disk_boundary_nodes};
use geode_core::assembly::magnetostatic::recover_b_field;
use geode_core::assembly::magnetostatic_nonlinear::{
    ConstantLaw, MaterialMap, NU_0, NewtonConfig, NonlinearMagnetostaticSystem, ReluctivityLaw,
    TabulatedBhLaw,
};

// ─────────────────────────────────────────────────────────────────────
// Fixture: the M27 table from the committed reference extraction
// ─────────────────────────────────────────────────────────────────────

fn m27_table() -> (Vec<f64>, Vec<f64>) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/motor_reference/reference_data.json"
    );
    let text = std::fs::read_to_string(path).expect("read reference_data.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse reference_data.json");
    let arr = |key: &str| -> Vec<f64> {
        v["material"][key]
            .as_array()
            .unwrap_or_else(|| panic!("material.{key} missing"))
            .iter()
            .map(|x| x.as_f64().expect("numeric table entry"))
            .collect()
    };
    let (b, h) = (arr("B_T"), arr("H_A_per_m"));
    assert_eq!(b.len(), 47, "M27 table must have 47 points");
    assert_eq!(h.len(), 47, "M27 table must have 47 points");
    (b, h)
}

fn m27() -> TabulatedBhLaw {
    let (b, h) = m27_table();
    TabulatedBhLaw::from_hb(&h, &b).expect("M27 table builds a valid law")
}

/// Dense B grid on [0, b_max] with `n` uniform steps; the offset keeps most
/// samples off the knots (multiples of 0.05 T), and the knots themselves are
/// appended.
fn dense_grid(b_max: f64, n: usize, knots: &[f64]) -> Vec<f64> {
    let mut g: Vec<f64> = (0..=n)
        .map(|i| (b_max * i as f64 / n as f64 + 3.7e-7).min(b_max))
        .collect();
    g.push(0.0);
    g.extend_from_slice(knots);
    g
}

// ─────────────────────────────────────────────────────────────────────
// 1–3. Construction, knots, positivity, non-monotone ν
// ─────────────────────────────────────────────────────────────────────

#[test]
fn m27_law_builds_interpolates_and_has_positive_tangent() {
    let (b, h) = m27_table();
    let law = m27();
    println!("\n[M27-1] TabulatedBhLaw on the M27 table (47 points, 0..2.30 T)");
    let nu0 = law.initial_reluctivity();
    println!(
        "    nu(0) = {nu0:.6} m/H (initial mu_r = {:.1}); last secant = {:.6e} m/H; 1/mu0 = {NU_0:.6e} m/H",
        1.0 / (nu0 * 4.0e-7 * std::f64::consts::PI),
        (h[46] - h[45]) / (b[46] - b[45])
    );

    // Exact at the knots.
    let mut worst_knot = 0.0_f64;
    for (&bk, &hk) in b.iter().zip(&h) {
        let e = (law.h_of_b(bk) - hk).abs() / hk.max(1.0);
        worst_knot = worst_knot.max(e);
    }
    println!("    max |H(B_k) - H_k| / max(H_k, 1) over the 47 knots = {worst_knot:.3e}");
    assert!(worst_knot <= 1e-14, "not interpolated: {worst_knot:e}");

    // Positivity on a dense grid through the tail.
    let grid = dense_grid(3.0, 300_000, &b);
    let mut min_dh = f64::INFINITY;
    let mut min_nu = f64::INFINITY;
    let mut min_tan = f64::INFINITY;
    let mut min_tan_at = 0.0;
    for &bb in &grid {
        let s = bb * bb;
        let nu = law.nu(s);
        let tan = nu + 2.0 * s * law.dnu_db2(s);
        let dh = law.dh_db(bb);
        assert!(dh > 0.0, "dH/dB = {dh} <= 0 at B = {bb}");
        assert!(nu > 0.0, "nu = {nu} <= 0 at B = {bb}");
        assert!(tan > 0.0, "nu + 2 s nu' = {tan} <= 0 at B = {bb}");
        min_dh = min_dh.min(dh);
        min_nu = min_nu.min(nu);
        if tan < min_tan {
            min_tan = tan;
            min_tan_at = bb;
        }
        // dh_db and nu + 2 s nu' are the same quantity by two routes.
        assert!(
            (tan - dh).abs() <= 1e-9 * dh,
            "dH/dB routes disagree at B = {bb}: {tan} vs {dh}"
        );
    }
    println!(
        "    dense grid ({} points, 0..3 T): min dH/dB = {min_dh:.6} m/H; min nu = {min_nu:.6} m/H; \
         min (nu + 2 s nu') = {min_tan:.6} m/H at B = {min_tan_at:.4} T",
        grid.len()
    );

    // ν'(s) < 0 at low field: M27's H/B falls from 341 to 256 m/H between
    // 0.05 and 0.10 T. Monotone ν is not required; positive dH/dB is.
    let nu_k = |k: usize| h[k] / b[k];
    println!(
        "    knot H/B: B=0.05 -> {:.3}, B=0.10 -> {:.3}, B=0.15 -> {:.3} m/H",
        nu_k(1),
        nu_k(2),
        nu_k(3)
    );
    assert!(
        nu_k(1) > nu_k(2),
        "M27 H/B is expected to fall at low field"
    );
    let mut neg_lo = f64::NAN;
    let mut neg_hi = f64::NAN;
    let mut most_neg = 0.0_f64;
    let mut sign_changes = 0usize;
    let mut prev_neg = law.dnu_db2(0.0) < 0.0;
    for i in 1..=30_000 {
        let bb = 1.5 * i as f64 / 30_000.0;
        let d = law.dnu_db2(bb * bb);
        if (d < 0.0) != prev_neg {
            sign_changes += 1;
            prev_neg = d < 0.0;
        }
        if d < 0.0 {
            if neg_lo.is_nan() {
                neg_lo = bb;
            }
            neg_hi = bb;
            most_neg = most_neg.min(d);
        }
    }
    println!(
        "    nu'(s) < 0 from B = {neg_lo:.5} to {neg_hi:.5} T (30000 samples on 0..1.5 T, \
         {sign_changes} sign change(s) of nu'); most negative nu' = {most_neg:.4} m/(H T^2); \
         nu'(0) = {:.4}",
        law.dnu_db2(0.0)
    );
    assert!(
        !neg_lo.is_nan() && neg_lo < 0.1,
        "expected nu'(s) < 0 at low field on M27"
    );
    assert!(law.dnu_db2(0.0) < 0.0);

    // C¹ join to the 1/μ₀ tail at the last knot.
    let b_last = b[46];
    let left = law.dh_db(b_last - 1e-9);
    println!(
        "    last-knot slope: left {left:.6e}, tail {:.6e} (1/mu0)",
        law.dh_db(b_last + 1e-9)
    );
    assert!((left - NU_0).abs() < 1e-5 * NU_0, "tail join not C1");
    assert!((law.dh_db(b_last + 0.5) - NU_0).abs() < 1e-9 * NU_0);
}

// ─────────────────────────────────────────────────────────────────────
// 4. FD checks of dν/d(b²) and of the energy density
// ─────────────────────────────────────────────────────────────────────

#[test]
fn m27_law_derivatives_match_finite_difference() {
    let law = m27();
    println!("\n[M27-2] central FD (step 1e-6 relative in b^2) on the M27 law, 0..3 T");
    let mut worst_dnu = 0.0_f64;
    let mut worst_dnu_at = 0.0;
    let mut worst_w = 0.0_f64;
    let n = 3000;
    for i in 0..n {
        // Offset keeps the FD stencil off the knots (multiples of 0.05 T).
        let bb = 3.0 * i as f64 / n as f64 + 4.1e-4;
        let b2 = bb * bb;
        let d = 1e-6 * b2;
        let fd_nu = (law.nu(b2 + d) - law.nu(b2 - d)) / (2.0 * d);
        let an = law.dnu_db2(b2);
        let scale = an.abs() * b2 + law.nu(b2);
        let e = (fd_nu - an).abs() * b2 / scale;
        if e > worst_dnu {
            worst_dnu = e;
            worst_dnu_at = bb;
        }
        let fd_w = (law.energy_density(b2 + d) - law.energy_density(b2 - d)) / (2.0 * d);
        worst_w = worst_w.max((fd_w - 0.5 * law.nu(b2)).abs() / law.nu(b2));
    }
    println!(
        "    max |FD(nu) - dnu/db2|·b2/(nu + |nu'| b2) = {worst_dnu:.3e} (at B = {worst_dnu_at:.4} T); \
         max |FD(w) - nu/2|/nu = {worst_w:.3e}"
    );
    assert!(worst_dnu < 1e-7, "dnu_db2 FD mismatch {worst_dnu:e}");
    assert!(worst_w < 1e-7, "energy-density FD mismatch {worst_w:e}");

    // Energy at the knots against an independent composite Simpson rule on
    // H(B) (the law is a piecewise cubic, so Simpson is exact per segment).
    let (b, _) = m27_table();
    let mut worst_e = 0.0_f64;
    let mut acc = 0.0;
    for k in 0..46 {
        let (lo, hi) = (b[k], b[k + 1]);
        let mid = 0.5 * (lo + hi);
        acc += (hi - lo) / 6.0 * (law.h_of_b(lo) + 4.0 * law.h_of_b(mid) + law.h_of_b(hi));
        let w = law.energy_density(hi * hi);
        worst_e = worst_e.max((w - acc).abs() / acc);
    }
    println!("    knot energy vs Simpson on H(B): max rel diff = {worst_e:.3e}");
    assert!(worst_e < 1e-12, "knot energy mismatch {worst_e:e}");
}

// ─────────────────────────────────────────────────────────────────────
// 5. Saturated-iron Ampère coax with M27 iron
// ─────────────────────────────────────────────────────────────────────

const TAG_COND: i32 = 0;
const TAG_GAP: i32 = 1;
const TAG_IRON: i32 = 2;
const TAG_AIR: i32 = 3;
const R_COND: f64 = 0.004;
const R_IRON_IN: f64 = 0.005;
const R_IRON_OUT: f64 = 0.25;
const R_OUT: f64 = 0.30;
/// Field strength at the inner iron radius (A/m). On M27 this is about
/// 2.06 T (deep saturation); the outer radius sits at 800 A/m, about 1.49 T
/// (the knee).
const H_INNER: f64 = 4.0e4;

fn coax_tag(r: f64) -> i32 {
    if r < R_COND {
        TAG_COND
    } else if r < R_IRON_IN {
        TAG_GAP
    } else if r < R_IRON_OUT {
        TAG_IRON
    } else {
        TAG_AIR
    }
}

fn centroid(mesh: &TriMesh, t: &[u32; 3]) -> [f64; 2] {
    let p = |i: usize| mesh.nodes[t[i] as usize];
    [
        (p(0)[0] + p(1)[0] + p(2)[0]) / 3.0,
        (p(0)[1] + p(1)[1] + p(2)[1]) / 3.0,
    ]
}

fn tri_area(mesh: &TriMesh, t: &[u32; 3]) -> f64 {
    let [a, b, c] = [
        mesh.nodes[t[0] as usize],
        mesh.nodes[t[1] as usize],
        mesh.nodes[t[2] as usize],
    ];
    0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs()
}

/// Concentric-ring disk mesh (same construction as
/// `tests/magnetostatic_nonlinear.rs`).
fn ring_mesh(ring_r: &[f64], n_ang: usize) -> (TriMesh, Vec<i32>) {
    let mut nodes = vec![[0.0, 0.0]];
    for &r in &ring_r[1..] {
        for s in 0..n_ang {
            let th = TAU * s as f64 / n_ang as f64;
            nodes.push([r * th.cos(), r * th.sin()]);
        }
    }
    let rn = |g: usize, s: usize| (1 + (g - 1) * n_ang + s % n_ang) as u32;
    let mut tris = Vec::new();
    for s in 0..n_ang {
        tris.push([0, rn(1, s), rn(1, s + 1)]);
    }
    for g in 1..ring_r.len() - 1 {
        for s in 0..n_ang {
            let (a, b, c, d) = (rn(g, s), rn(g, s + 1), rn(g + 1, s + 1), rn(g + 1, s));
            tris.push([a, d, c]);
            tris.push([a, c, b]);
        }
    }
    let mesh = TriMesh { nodes, tris };
    let tags = mesh
        .tris
        .iter()
        .map(|t| {
            let c = centroid(&mesh, t);
            coax_tag((c[0] * c[0] + c[1] * c[1]).sqrt())
        })
        .collect();
    (mesh, tags)
}

fn coax_mesh(n_ang: usize) -> (TriMesh, Vec<i32>) {
    let dth = TAU / n_ang as f64;
    let mut r = vec![0.0];
    let n_cond = (n_ang / 8).max(3);
    for k in 1..=n_cond {
        r.push(R_COND * k as f64 / n_cond as f64);
    }
    let push_log = |r: &mut Vec<f64>, a: f64, b: f64, min_n: usize| {
        let n = (((b / a).ln() / dth).round() as usize).max(min_n);
        for k in 1..=n {
            r.push(a * (b / a).powf(k as f64 / n as f64));
        }
    };
    push_log(&mut r, R_COND, R_IRON_IN, 2);
    push_log(&mut r, R_IRON_IN, R_IRON_OUT, 4);
    push_log(&mut r, R_IRON_OUT, R_OUT, 2);
    ring_mesh(&r, n_ang)
}

fn coax_system(
    mesh: &TriMesh,
    tags: &[i32],
    i_total: f64,
    iron: impl ReluctivityLaw + 'static,
) -> NonlinearMagnetostaticSystem {
    let cond_area: f64 = mesh
        .tris
        .iter()
        .zip(tags)
        .filter(|(_, g)| **g == TAG_COND)
        .map(|(t, _)| tri_area(mesh, t))
        .sum();
    let j: Vec<f64> = tags
        .iter()
        .map(|&g| {
            if g == TAG_COND {
                i_total / cond_area
            } else {
                0.0
            }
        })
        .collect();
    let mats = MaterialMap::new()
        .with(TAG_COND, ConstantLaw::vacuum())
        .with(TAG_GAP, ConstantLaw::vacuum())
        .with(TAG_IRON, iron)
        .with(TAG_AIR, ConstantLaw::vacuum());
    let bc = disk_boundary_nodes(mesh, R_OUT);
    NonlinearMagnetostaticSystem::new(mesh, tags, &mats, &j, &bc).expect("coax system")
}

/// (max rel err, RMS rel err, mid-annulus mean rel err, |B| min, |B| max)
/// of the element |B| against the exact oracle in the iron.
fn oracle_errors(mesh: &TriMesh, tags: &[i32], a_z: &[f64], i: f64) -> [f64; 5] {
    let law = m27();
    let b = recover_b_field(mesh, a_z);
    let r_mid = (R_IRON_IN * R_IRON_OUT).sqrt();
    let (mut max_rel, mut sum2, mut cnt, mut mid_sum, mut mid_cnt) = (0.0_f64, 0.0, 0usize, 0.0, 0);
    let (mut b_lo, mut b_hi) = (f64::INFINITY, 0.0_f64);
    for (t, tri) in mesh.tris.iter().enumerate() {
        if tags[t] != TAG_IRON {
            continue;
        }
        let c = centroid(mesh, tri);
        let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
        let b_exact = law.b_of_h(i / (TAU * r)).unwrap();
        let bh = (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
        let rel = (bh - b_exact).abs() / b_exact;
        max_rel = max_rel.max(rel);
        sum2 += rel * rel;
        cnt += 1;
        if (r / r_mid).ln().abs() < 0.1 {
            mid_sum += rel;
            mid_cnt += 1;
        }
        b_lo = b_lo.min(b_exact);
        b_hi = b_hi.max(b_exact);
    }
    [
        max_rel,
        (sum2 / cnt as f64).sqrt(),
        mid_sum / mid_cnt as f64,
        b_lo,
        b_hi,
    ]
}

#[test]
fn m27_saturated_iron_ampere_oracle() {
    let i = TAU * R_IRON_IN * H_INNER;
    println!(
        "\n[M27-3] saturated-iron Ampere coax with M27 iron: |B|(r) = B_of_H(I/(2 pi r)); I = {i:.2} A"
    );
    println!(
        "    n_ang | nodes  | Newton its | status        | max rel err | RMS rel err | mid-annulus mean rel err | |B| in iron (T)"
    );
    let mut rows = Vec::new();
    for n_ang in [32usize, 64, 128, 256] {
        let (mesh, tags) = coax_mesh(n_ang);
        let sys = coax_system(&mesh, &tags, i, m27());
        let sol = sys
            .solve(&NewtonConfig::default(), None)
            .expect("M27 coax Newton");
        let e = oracle_errors(&mesh, &tags, &sol.a_z, i);
        println!(
            "    {n_ang:5} | {:6} | {:10} | {:13} | {:11.4e} | {:11.4e} | {:24.4e} | {:.3} .. {:.3}",
            mesh.n_nodes(),
            sol.iterations,
            format!("{:?}", sol.status),
            e[0],
            e[1],
            e[2],
            e[3],
            e[4]
        );
        rows.push(e);
    }
    let first = rows[0];
    let last = rows[rows.len() - 1];
    assert!(
        first[4] > 2.0,
        "inner iron not deeply saturated: {}",
        first[4]
    );
    assert!(first[3] < 1.55, "outer iron not in the knee: {}", first[3]);
    for w in rows.windows(2) {
        assert!(
            w[1][1] < w[0][1],
            "RMS error not decreasing with refinement"
        );
    }
    assert!(
        last[2] <= 0.01,
        "finest mid-annulus |B| error {:.4e} exceeds the 1% target",
        last[2]
    );

    // Tripwire: linear iron at the M27 initial reluctivity misses the
    // saturated oracle by far more than the nonlinear solve.
    let (mesh, tags) = coax_mesh(32);
    let lin_law = ConstantLaw::new(m27().initial_reluctivity()).unwrap();
    let lin = coax_system(&mesh, &tags, i, lin_law)
        .solve(&NewtonConfig::default(), None)
        .unwrap();
    let trip = oracle_errors(&mesh, &tags, &lin.a_z, i)[2];
    println!(
        "    tripwire: linear iron at nu(0) (n_ang=32) misses the mid-annulus oracle by {trip:.3e} \
         (nonlinear: {:.3e})",
        first[2]
    );
    assert!(
        trip > 100.0 * first[2] && trip > 0.1,
        "tripwire too weak: {trip:e}"
    );
}
