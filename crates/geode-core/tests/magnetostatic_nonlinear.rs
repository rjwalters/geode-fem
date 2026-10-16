//! Acceptance tests for the **nonlinear** B-H scalar magnetostatic solver
//! (motor-paper milestone M1, issue #1157):
//! `geode_core::assembly::magnetostatic_nonlinear`.
//!
//! 1. **Linear limit**: constant laws reproduce `assemble_magnetostatic` +
//!    `solve` (relative-ν convention) to ≤1e-12, in one Newton step. This
//!    pins the SI ↔ relative unit mapping, including the PM source.
//! 2. **Tangent check**: the central-difference directional derivative of
//!    `R` matches `K·v` at a saturated state, over a 1e-2 … 1e-8 step sweep,
//!    for both laws. `dν/d(b²)` and `w(b²)` are also checked against FD of
//!    `ν`, and the SPD tangent is checked.
//! 3. **Quadratic convergence** of Newton on a deeply saturated case,
//!    contrasted with the linear convergence of the frozen-ν fixed point.
//! 4. **Manufactured solution** with Brauer ν and non-homogeneous Dirichlet
//!    data: O(h) in the H¹ seminorm and O(h²) in L².
//! 5. **Saturated-iron Ampère oracle**: coaxial conductor / iron annulus /
//!    air, with `|B|(r) = B(I/(2πr))` exact in the iron.
//! 6. **Law validation**: malformed tables and parameters are rejected.
//! 7. **Warm starts and the round-off floor**: restarts and 0.1% current
//!    re-solves from a previous solution succeed, and an unreachable
//!    tolerance ends at the floor (Judge finding B1, PR #1159).
//!
//! Every test prints its measured table. The committed
//! `benchmarks/motor_nonlinear_m1/m1_tables.log` is this file's
//! `--nocapture` output; see that directory's README.
//!
//! The B-H table used here is **synthetic**: a realistic non-oriented
//! electrical-steel shape, not measured data and not tied to any grade.
#![allow(clippy::needless_range_loop)]

use std::f64::consts::{PI, TAU};

use faer::Side;
use geode_core::analytic::waveguide::{
    RadialGrading, TRI_QUAD_DEG4, TriMesh, disk_boundary_nodes, disk_tri_mesh_bands, rect_tri_mesh,
};
use geode_core::assembly::magnetostatic::{
    assemble_magnetostatic, assemble_magnetostatic_pm, build_nu_r, radial_magnetization_source,
    recover_b_field,
};
use geode_core::assembly::magnetostatic_nonlinear::{
    BrauerLaw, ConstantLaw, ConvergenceStatus, LineSearch, MaterialMap, NU_0, NewtonConfig,
    NonlinearMagnetostaticError, NonlinearMagnetostaticSystem, ReluctivityLaw, TabulatedBhLaw,
    TangentKind, load_vector_from_fn,
};

const MU_0: f64 = 4.0e-7 * PI;

// ─────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────

/// **Synthetic** non-oriented electrical-steel B-H table (H in A/m, B in T).
/// Initial μ_r ≈ 5000, knee at about 1.3–1.5 T, deep saturation above
/// 1.8 T. It reaches 2.28 T at 200 kA/m, where the last secant (≈5.6e5 m/H)
/// is close enough to 1/μ₀ for a monotone C¹ join to the saturation tail.
/// This is not measured data.
const SYNTH_H: [f64; 16] = [
    0.0, 40.0, 80.0, 120.0, 180.0, 250.0, 400.0, 700.0, 1500.0, 3000.0, 6000.0, 12000.0, 25000.0,
    50000.0, 100000.0, 200000.0,
];
const SYNTH_B: [f64; 16] = [
    0.0, 0.25, 0.55, 0.85, 1.05, 1.18, 1.30, 1.40, 1.50, 1.58, 1.66, 1.75, 1.86, 1.97, 2.10, 2.28,
];

fn synthetic_steel() -> TabulatedBhLaw {
    TabulatedBhLaw::from_hb(&SYNTH_H, &SYNTH_B).expect("synthetic table is valid")
}

/// Representative Brauer coefficients for electrical steel in SI
/// (`ν = 3.8·exp(2.17 b²) + 396.2` m/H), so μ_r ≈ 2000 at B = 0 and
/// μ_r ≈ 35 at 2 T.
fn brauer() -> BrauerLaw {
    BrauerLaw::new(3.8, 2.17, 396.2).expect("valid Brauer law")
}

/// Deterministic LCG in [-1, 1).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Concentric-ring disk mesh with explicit ring radii (`ring_r[0] == 0`),
/// `n_ang` sectors, CCW triangles and a centroid-radius tag.
fn ring_mesh(ring_r: &[f64], n_ang: usize, tag_of: impl Fn(f64) -> i32) -> (TriMesh, Vec<i32>) {
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
            tag_of((c[0] * c[0] + c[1] * c[1]).sqrt())
        })
        .collect();
    (mesh, tags)
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

// Coaxial Ampère geometry (SI, metres).
const TAG_COND: i32 = 0;
const TAG_GAP: i32 = 1;
const TAG_IRON: i32 = 2;
const TAG_AIR: i32 = 3;
const R_COND: f64 = 0.004;
const R_IRON_IN: f64 = 0.005;
const R_IRON_OUT: f64 = 0.25;
const R_OUT: f64 = 0.30;
/// Field strength at the inner iron radius (A/m): B ≈ 1.92 T on the table.
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

/// Coax mesh at angular resolution `n_ang`; radial steps track the angular
/// step (`Δr/r ≈ 2π/n_ang`) through the log-graded iron annulus.
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
    ring_mesh(&r, n_ang, coax_tag)
}

/// Build the coax system for total current `i_total` (A); the current
/// density is normalized by the meshed conductor area so `I_enc` is exact.
fn coax_system(mesh: &TriMesh, tags: &[i32], i_total: f64) -> NonlinearMagnetostaticSystem {
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
        .with(TAG_IRON, synthetic_steel())
        .with(TAG_AIR, ConstantLaw::vacuum());
    let bc = disk_boundary_nodes(mesh, R_OUT);
    NonlinearMagnetostaticSystem::new(mesh, tags, &mats, &j, &bc).expect("coax system")
}

fn coax_current() -> f64 {
    TAU * R_IRON_IN * H_INNER
}

// ─────────────────────────────────────────────────────────────────────
// 1. Linear limit
// ─────────────────────────────────────────────────────────────────────

/// Constant laws must reproduce the relative-ν linear module exactly:
/// `ν_SI = ν_r/μ₀`, `J_SI = j̃/μ₀`, and the PM source divided by `μ₀`
/// all give the same `A_z`. Newton must converge in one step.
#[test]
fn linear_limit_matches_linear_module() {
    let radii = [0.0, 0.030, 0.040, 0.045, 0.055, 0.20];
    let n_rad = [8usize, 8, 6, 8, 10];
    let (mesh, tags) = disk_tri_mesh_bands(&radii, 96, &n_rad, &[RadialGrading::Uniform; 5]);
    // Iron bore, recoil-permeability magnet, air gap, winding, outer air.
    let mu_r = [1000.0, 1.05, 1.0, 1.0, 1.0];
    let bc = disk_boundary_nodes(&mesh, 0.20);
    let p = 2u32;
    let j0 = 4.0e7; // A/m², SI
    let j_si: Vec<f64> = mesh
        .tris
        .iter()
        .zip(&tags)
        .map(|(t, &g)| {
            if g == 3 {
                let c = centroid(&mesh, t);
                j0 * (p as f64 * c[1].atan2(c[0])).cos()
            } else {
                0.0
            }
        })
        .collect();
    let m = radial_magnetization_source(&mesh, &tags, 1, 1.2 / MU_0, p);

    let mut mats = MaterialMap::new();
    for (tag, &mr) in mu_r.iter().enumerate() {
        mats.insert(
            tag as i32,
            ConstantLaw::from_relative_permeability(mr).unwrap(),
        );
    }

    println!("\n[1] linear limit (constant laws vs relative-nu linear module)");
    println!("    case             | max|dA|/max|A| | Newton iters | |R1|/|R0|");
    for (label, with_pm) in [("current only", false), ("current + PM", true)] {
        // Linear module, relative convention: ν_r = 1/μ_r, j̃ = μ₀ J.
        let nu_r = build_nu_r(&tags, &mu_r);
        let j_rel: Vec<f64> = j_si.iter().map(|j| MU_0 * j).collect();
        let lin = if with_pm {
            assemble_magnetostatic_pm(&mesh, &nu_r, &j_rel, &m, &bc).unwrap()
        } else {
            assemble_magnetostatic(&mesh, &nu_r, &j_rel, &bc).unwrap()
        };
        let a_lin = lin.solve().unwrap();

        let mut sys = NonlinearMagnetostaticSystem::new(&mesh, &tags, &mats, &j_si, &bc).unwrap();
        if with_pm {
            sys = sys.with_magnetization(&m).unwrap();
        }
        let sol = sys.solve(&NewtonConfig::default(), None).unwrap();

        let scale = a_lin.iter().fold(0.0_f64, |a, x| a.max(x.abs()));
        let diff = a_lin
            .iter()
            .zip(&sol.a_z)
            .fold(0.0_f64, |a, (x, y)| a.max((x - y).abs()));
        let rel = diff / scale;
        let r_ratio = sol.residual_norms[1] / sol.residual_norms[0];
        println!(
            "    {label:<16} | {rel:14.3e} | {:12} | {r_ratio:.3e}",
            sol.iterations
        );
        assert!(scale > 0.0);
        assert!(
            rel <= 1e-12,
            "{label}: linear-limit mismatch {rel:e} > 1e-12"
        );
        assert_eq!(
            sol.iterations, 1,
            "{label}: Newton must converge in one step"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// 2. Tangent consistency
// ─────────────────────────────────────────────────────────────────────

/// A saturated all-iron state on the square `[0, L]²`, `L = 5 cm`:
/// `A = s·L·e^{y/L} sin(2x/L + 0.5)`, so `|B| ∈ [s, 2e·s]` spans the knee,
/// deep saturation and (for the table at `s = 0.5`) the 1/μ₀ tail.
fn saturated_square_state(
    law: impl ReluctivityLaw + 'static,
    n: usize,
    b_scale: f64,
) -> (TriMesh, NonlinearMagnetostaticSystem, Vec<f64>) {
    let l = 0.05;
    let mesh = rect_tri_mesh(n, n, l, l);
    let tags = vec![0; mesh.n_tris()];
    let mats = MaterialMap::new().with(0, law);
    let tol = 1e-12 * l;
    let bc: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| p[0] < tol || p[1] < tol || p[0] > l - tol || p[1] > l - tol)
        .collect();
    let j = vec![0.0; mesh.n_tris()];
    let a: Vec<f64> = mesh
        .nodes
        .iter()
        .map(|p| b_scale * l * (p[1] / l).exp() * (2.0 * p[0] / l + 0.5).sin())
        .collect();
    let sys = NonlinearMagnetostaticSystem::new(&mesh, &tags, &mats, &j, &bc)
        .unwrap()
        .with_dirichlet_values(&a)
        .unwrap();
    (mesh, sys, a)
}

/// Per-element `|∇u|` of a full nodal vector.
fn elem_grad_norms(mesh: &TriMesh, u: &[f64]) -> Vec<f64> {
    recover_b_field(mesh, u)
        .iter()
        .map(|b| (b[0] * b[0] + b[1] * b[1]).sqrt())
        .collect()
}

struct FdRow {
    h: f64,
    err_all: f64,
    err_smooth: f64,
    n_cross: usize,
}

/// Central-difference check of `K·v` against `(R(A+hsv) − R(A−hsv))/(2hs)`.
///
/// `s` scales the random direction so that its largest element gradient
/// equals the state's largest `|B|`: `h` is the relative perturbation of
/// the field. `knots` are the law's C¹ break points in `|B|`, where `H''`
/// (and so the derivative of `ν'`) jumps. Elements whose `|B|` crosses a
/// knot between `A − hsv` and `A + hsv` have an O(h) local FD truncation
/// error. `err_smooth` drops the rows touched by such elements, which
/// isolates the O(h²) regime of the rest.
fn fd_tangent_sweep(
    label: &str,
    mesh: &TriMesh,
    sys: &NonlinearMagnetostaticSystem,
    a: &[f64],
    knots: &[f64],
) -> Vec<FdRow> {
    let (_r, k) = sys
        .residual_and_tangent(a, TangentKind::Consistent)
        .unwrap();
    let a_free = sys.restrict(a);
    let mut rng = Lcg(0x1157);
    let v_free: Vec<f64> = (0..sys.n_free()).map(|_| rng.next()).collect();
    let mut v_full = vec![0.0; mesh.n_nodes()];
    for (g, slot) in sys.free_of_global().iter().enumerate() {
        if let Some(fi) = slot {
            v_full[g] = v_free[*fi];
        }
    }
    let b_state = elem_grad_norms(mesh, a);
    let b_max = b_state.iter().copied().fold(0.0_f64, f64::max);
    let s = b_max
        / elem_grad_norms(mesh, &v_full)
            .iter()
            .copied()
            .fold(0.0_f64, f64::max);
    let v: Vec<f64> = v_free.iter().map(|x| x * s).collect();
    let kv: Vec<f64> = {
        let mut out = vec![0.0; sys.n_free()];
        let kr = k.as_ref();
        for j in 0..sys.n_free() {
            for idx in kr.col_ptr()[j]..kr.col_ptr()[j + 1] {
                out[kr.row_idx()[idx]] += kr.val()[idx] * v[j];
            }
        }
        out
    };
    let kv_norm = norm(&kv);
    let mut rows = Vec::new();
    let mut touched = vec![false; sys.n_free()];
    println!(
        "    {label}: |B| in [{:.3}, {b_max:.3}] T; step h = relative field perturbation",
        b_state.iter().copied().fold(f64::INFINITY, f64::min)
    );
    println!(
        "        h    | ||FD-Kv||/||Kv|| (all rows) | knot-crossing elems | same, rows off the h=1e-2 knot-crossing elems"
    );
    for e in 2..=8 {
        let h = 10f64.powi(-e);
        let plus = sys.lift(
            &a_free
                .iter()
                .zip(&v)
                .map(|(x, d)| x + h * d)
                .collect::<Vec<_>>(),
        );
        let minus = sys.lift(
            &a_free
                .iter()
                .zip(&v)
                .map(|(x, d)| x - h * d)
                .collect::<Vec<_>>(),
        );
        let rp = sys.residual(&plus);
        let rm = sys.residual(&minus);
        let diff: Vec<f64> = rp
            .iter()
            .zip(&rm)
            .zip(&kv)
            .map(|((p, m), k)| (p - m) / (2.0 * h) - k)
            .collect();
        let err_all = norm(&diff) / kv_norm;
        // Knot-crossing elements at this h. The excluded row set is fixed
        // at the largest h (it contains the sets of all smaller steps), so
        // the knot-free column compares the same rows at every h.
        let (bp, bm) = (elem_grad_norms(mesh, &plus), elem_grad_norms(mesh, &minus));
        let mut n_cross = 0;
        for (t, tri) in mesh.tris.iter().enumerate() {
            let lo = b_state[t].min(bp[t]).min(bm[t]);
            let hi = b_state[t].max(bp[t]).max(bm[t]);
            if knots.iter().any(|&kb| kb >= lo && kb <= hi) {
                n_cross += 1;
                if e == 2 {
                    for &p in tri {
                        if let Some(fi) = sys.free_of_global()[p as usize] {
                            touched[fi] = true;
                        }
                    }
                }
            }
        }
        let (mut num, mut den) = (0.0, 0.0);
        for i in 0..sys.n_free() {
            if !touched[i] {
                num += diff[i] * diff[i];
                den += kv[i] * kv[i];
            }
        }
        let err_smooth = (num / den).sqrt();
        println!("      {h:7.0e} | {err_all:27.3e} | {n_cross:19} | {err_smooth:.3e}");
        rows.push(FdRow {
            h,
            err_all,
            err_smooth,
            n_cross,
        });
    }
    rows
}

fn order(e0: f64, e1: f64, h0: f64, h1: f64) -> f64 {
    (e0 / e1).log10() / (h0 / h1).log10()
}

/// Best error ≤ 1e-7 at the best step, and the O(h²) regime over the
/// first three decades (before round-off), measured on the knot-free rows.
fn assert_tangent_rows(label: &str, rows: &[FdRow]) {
    let best = rows.iter().map(|r| r.err_all).fold(f64::INFINITY, f64::min);
    let best_h = rows.iter().find(|r| r.err_all == best).unwrap().h;
    println!("      {label}: best ||FD-Kv||/||Kv|| = {best:.3e} at h = {best_h:.0e}");
    assert!(
        best <= 1e-7,
        "{label}: best FD tangent error {best:e} > 1e-7"
    );
    for w in rows[..4].windows(2) {
        let p_all = order(w[0].err_all, w[1].err_all, w[0].h, w[1].h);
        let p_s = order(w[0].err_smooth, w[1].err_smooth, w[0].h, w[1].h);
        println!(
            "      {label}: order h={:.0e} -> {:.0e}: all rows {p_all:.2}, knot-free rows {p_s:.2}",
            w[0].h, w[1].h
        );
        assert!(
            (1.8..=2.2).contains(&p_s),
            "{label}: knot-free FD error not O(h^2) between h={} and h={}: order {p_s:.3}",
            w[0].h,
            w[1].h
        );
    }
}

#[test]
fn tangent_matches_finite_difference_brauer() {
    println!(
        "\n[2a] consistent-tangent FD check, Brauer law, saturated state (16x16 square, 5 cm)"
    );
    let (mesh, sys, a) = saturated_square_state(brauer(), 16, 0.4);
    let nu = sys.element_reluctivity(&a);
    let (lo, hi) = nu
        .iter()
        .fold((f64::INFINITY, 0.0_f64), |(l, h), &x| (l.min(x), h.max(x)));
    println!(
        "    element nu range: {lo:.4e} .. {hi:.4e} m/H (mu_r {:.0} .. {:.0})",
        1.0 / (MU_0 * hi),
        1.0 / (MU_0 * lo)
    );
    let rows = fd_tangent_sweep("Brauer", &mesh, &sys, &a, &[]);
    assert!(rows.iter().all(|r| r.n_cross == 0));
    assert_tangent_rows("Brauer", &rows);
}

#[test]
fn tangent_matches_finite_difference_table() {
    println!(
        "\n[2b] consistent-tangent FD check, synthetic B-H table, saturated state (16x16 square, 5 cm)"
    );
    let law = synthetic_steel();
    let knots = law.b_points()[1..].to_vec();
    let (mesh, sys, a) = saturated_square_state(law, 16, 0.5);
    let nu = sys.element_reluctivity(&a);
    let (lo, hi) = nu
        .iter()
        .fold((f64::INFINITY, 0.0_f64), |(l, h), &x| (l.min(x), h.max(x)));
    println!(
        "    element nu range: {lo:.4e} .. {hi:.4e} m/H (mu_r {:.0} .. {:.0})",
        1.0 / (MU_0 * hi),
        1.0 / (MU_0 * lo)
    );
    let rows = fd_tangent_sweep("table", &mesh, &sys, &a, &knots);
    assert_tangent_rows("table", &rows);
}

/// `dν/d(b²)` against a central FD of `ν`, and `dw/d(b²) = ν/2` against a
/// central FD of the energy density, over a sweep through every table
/// segment and the tail.
#[test]
fn law_derivatives_match_finite_difference() {
    println!("\n[2c] law derivative checks (central FD, step 1e-6 relative in b^2)");
    let laws: [(&str, Box<dyn ReluctivityLaw>); 3] = [
        ("Brauer", Box::new(brauer())),
        ("table", Box::new(synthetic_steel())),
        (
            "constant",
            Box::new(ConstantLaw::from_relative_permeability(1000.0).unwrap()),
        ),
    ];
    for (label, law) in &laws {
        let mut worst_dnu = 0.0_f64;
        let mut worst_w = 0.0_f64;
        for i in 1..=300 {
            let bb = 2.8 * i as f64 / 300.0 + 1.3e-3; // offset avoids exact knots
            let b2 = bb * bb;
            let d = 1e-6 * b2;
            let fd_nu = (law.nu(b2 + d) - law.nu(b2 - d)) / (2.0 * d);
            let an = law.dnu_db2(b2);
            // Mixed scale: relative to ν'·b² + ν (the size of dH/dB terms).
            let scale = an.abs() * b2 + law.nu(b2);
            worst_dnu = worst_dnu.max((fd_nu - an).abs() * b2 / scale);
            let fd_w = (law.energy_density(b2 + d) - law.energy_density(b2 - d)) / (2.0 * d);
            worst_w = worst_w.max((fd_w - 0.5 * law.nu(b2)).abs() / law.nu(b2));
        }
        println!(
            "    {label:<8}: max |FD(nu) - dnu/db2|·b2/(nu + |nu'| b2) = {worst_dnu:.3e}; \
             max |FD(w) - nu/2|/nu = {worst_w:.3e}"
        );
        assert!(
            worst_dnu < 1e-7,
            "{label}: dnu_db2 FD mismatch {worst_dnu:e}"
        );
        assert!(
            worst_w < 1e-7,
            "{label}: energy-density FD mismatch {worst_w:e}"
        );
    }
    // Table near the origin: ν(0) is the initial slope, dν/d(b²) bounded.
    let t = synthetic_steel();
    let nu0 = t.nu(0.0);
    println!(
        "    table nu(0) = {nu0:.4} m/H (mu_r,initial = {:.0}), dnu/db2(0) = {:.4}",
        1.0 / (MU_0 * nu0),
        t.dnu_db2(0.0)
    );
    assert!((nu0 - t.initial_reluctivity()).abs() < 1e-12);
    assert!(t.dnu_db2(0.0).is_finite() && t.dnu_db2(1e-20).is_finite());
    // The table is interpolated (passes through every point) and B(H)
    // inverts H(B).
    for (&h, &b) in SYNTH_H.iter().zip(&SYNTH_B) {
        assert!(
            (t.h_of_b(b) - h).abs() <= 1e-9 * h.max(1.0),
            "table not interpolated at B={b}"
        );
        if h > 0.0 {
            assert!(
                (t.b_of_h(h).unwrap() - b).abs() <= 1e-12,
                "b_of_h({h}) = {} != {b}",
                t.b_of_h(h).unwrap()
            );
        }
    }
    // Saturation tail: dH/dB = 1/μ₀ past the last point, C¹ at the join.
    let b_last = *SYNTH_B.last().unwrap();
    assert!((t.dh_db(b_last + 0.5) - NU_0).abs() < 1e-9 * NU_0);
    let left = t.dh_db(b_last - 1e-9);
    assert!(
        (left - NU_0).abs() < 1e-5 * NU_0,
        "tail join not C1: {left} vs {NU_0}"
    );
}

/// Monotone `H(B)` ⇒ convex energy ⇒ SPD tangent: check `dH/dB > 0` and
/// `ν > 0` on a dense sweep, then the smallest eigenvalue of the assembled
/// reduced tangent at a saturated state.
#[test]
fn tangent_is_spd_for_monotone_laws() {
    println!("\n[2d] SPD tangent check");
    let laws: [(&str, Box<dyn ReluctivityLaw>); 2] = [
        ("Brauer", Box::new(brauer())),
        ("table", Box::new(synthetic_steel())),
    ];
    for (label, law) in &laws {
        let min_dh = (0..=3000)
            .map(|i| law.dh_db(3.0 * i as f64 / 3000.0))
            .fold(f64::INFINITY, f64::min);
        let min_nu = (0..=3000)
            .map(|i| law.nu((3.0 * i as f64 / 3000.0).powi(2)))
            .fold(f64::INFINITY, f64::min);
        println!("    {label:<6}: min dH/dB on [0,3] T = {min_dh:.4e}, min nu = {min_nu:.4e}");
        assert!(min_dh > 0.0 && min_nu > 0.0);
    }
    let states = [
        ("Brauer", saturated_square_state(brauer(), 10, 0.4)),
        ("table", saturated_square_state(synthetic_steel(), 10, 0.5)),
    ];
    for (label, (_mesh, sys, a)) in &states {
        let (_r, k) = sys
            .residual_and_tangent(a, TangentKind::Consistent)
            .unwrap();
        let dense = k.to_dense();
        let n = dense.nrows();
        let mut asym = 0.0_f64;
        let mut maxabs = 0.0_f64;
        for i in 0..n {
            for j in 0..n {
                asym = asym.max((dense[(i, j)] - dense[(j, i)]).abs());
                maxabs = maxabs.max(dense[(i, j)].abs());
            }
        }
        let eig = dense
            .as_ref()
            .self_adjoint_eigenvalues(Side::Lower)
            .unwrap();
        let lmin = eig.iter().copied().fold(f64::INFINITY, f64::min);
        let lmax = eig.iter().copied().fold(0.0_f64, f64::max);
        println!(
            "    {label:<6} tangent ({n}x{n}): max asym/max|K| = {:.2e}, lambda_min = {lmin:.4e}, lambda_max = {lmax:.4e}",
            asym / maxabs
        );
        assert!(asym <= 1e-14 * maxabs, "{label}: tangent not symmetric");
        assert!(
            lmin > 0.0,
            "{label}: tangent not SPD (lambda_min = {lmin:e})"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────
// 3. Quadratic convergence vs frozen-ν fixed point
// ─────────────────────────────────────────────────────────────────────

/// Observed convergence orders `p_k = ln(r_{k+1}/r_k) / ln(r_k/r_{k−1})`.
fn orders(r: &[f64]) -> Vec<f64> {
    r.windows(3)
        .map(|w| (w[2] / w[1]).ln() / (w[1] / w[0]).ln())
        .collect()
}

/// Least-squares slope of `ln r_{k+1}` against `ln r_k`: the convergence
/// order over a window (2 for quadratic, 1 for linear).
fn loglog_order(r: &[f64]) -> f64 {
    fit_rate(&r[..r.len() - 1], &r[1..])
}

/// Print a Newton history and return the terminal full-step residuals that
/// lie above 100× the final (round-off floor) residual, normalized by R0.
fn newton_report(
    label: &str,
    sol: &geode_core::assembly::magnetostatic_nonlinear::NonlinearMagnetostaticSolution,
) -> Vec<f64> {
    let r0 = sol.residual_norms[0];
    println!("    {label}: {} iterations from zero", sol.iterations);
    println!(
        "      k | ||R_k||/||R_0|| | alpha_k | r_k/r_(k-1) | C_k = r_k/r_(k-1)^2 (normalized)"
    );
    for (k, &r) in sol.residual_norms.iter().enumerate() {
        if k == 0 {
            println!("      {k:2} | {:14.3e} |         |             |", 1.0);
            continue;
        }
        let p = sol.residual_norms[k - 1] / r0;
        println!(
            "      {k:2} | {:14.3e} | {:7.4} | {:11.3e} | {:.3e}",
            r / r0,
            sol.step_lengths[k - 1],
            r / sol.residual_norms[k - 1],
            (r / r0) / (p * p)
        );
    }
    let r_final = *sol.residual_norms.last().unwrap();
    // Trailing run of full steps (α = 1), as residual indices.
    let mut first_full = sol.iterations;
    while first_full > 0 && sol.step_lengths[first_full - 1] == 1.0 {
        first_full -= 1;
    }
    // Residual index `first_full` starts the full-step phase; keep its
    // last four residuals above the floor (three terminal transitions).
    let above: Vec<f64> = sol.residual_norms[first_full..]
        .iter()
        .copied()
        .filter(|&r| r > 100.0 * r_final)
        .map(|r| r / r0)
        .collect();
    above[above.len().saturating_sub(4)..].to_vec()
}

/// Quadratic convergence: over the last three full-step transitions above
/// the round-off floor, the residual falls by more than three decades while the
/// quadratic constant `C_k = r_k/r_{k−1}²` stays within a factor of 10 (it
/// grows like `1/r_k` for linear convergence), and the fitted order exceeds
/// 1.7.
fn assert_quadratic(label: &str, terminal: &[f64]) -> f64 {
    assert!(
        terminal.len() == 4,
        "{label}: too few terminal residuals {terminal:?}"
    );
    let c: Vec<f64> = terminal.windows(2).map(|w| w[1] / (w[0] * w[0])).collect();
    let (cmin, cmax) = c
        .iter()
        .fold((f64::INFINITY, 0.0_f64), |(a, b), &x| (a.min(x), b.max(x)));
    let decades = (terminal[0] / terminal[terminal.len() - 1]).log10();
    let fitted = loglog_order(terminal);
    println!(
        "    {label}: terminal full-step residuals {:?}; C_k in [{cmin:.3e}, {cmax:.3e}]; \
         {decades:.1} decades; fitted order {fitted:.2}; consecutive orders {:.2?}",
        terminal
            .iter()
            .map(|r| format!("{r:.3e}"))
            .collect::<Vec<_>>(),
        orders(terminal)
    );
    assert!(
        decades > 3.0,
        "{label}: terminal phase spans only {decades:.1} decades"
    );
    assert!(
        cmax / cmin < 10.0,
        "{label}: quadratic constant not bounded: {c:?}"
    );
    assert!(
        fitted > 1.7,
        "{label}: fitted terminal order {fitted:.3} is not quadratic"
    );
    fitted
}

#[test]
fn newton_converges_quadratically_frozen_nu_linearly() {
    println!(
        "\n[3] convergence: consistent Newton vs frozen-nu fixed point (coax, deep saturation)"
    );
    let (mesh, tags) = coax_mesh(64);
    let sys = coax_system(&mesh, &tags, coax_current());
    println!(
        "    mesh: {} nodes, {} triangles; I = {:.2} A (H(r_i) = {H_INNER:.0} A/m); default config \
         (energy line search, rel_tol 1e-8)",
        mesh.n_nodes(),
        mesh.n_tris(),
        coax_current()
    );
    let sol = sys
        .solve(&NewtonConfig::default(), None)
        .expect("Newton converges");
    let terminal = newton_report("Newton, synthetic table", &sol);
    let p_table = assert_quadratic("Newton, synthetic table", &terminal);

    // The same geometry and current with the smooth (C-infinity) Brauer law
    // in the iron.
    let cond_area: f64 = mesh
        .tris
        .iter()
        .zip(&tags)
        .filter(|(_, g)| **g == TAG_COND)
        .map(|(t, _)| tri_area(&mesh, t))
        .sum();
    let j: Vec<f64> = tags
        .iter()
        .map(|&g| {
            if g == TAG_COND {
                coax_current() / cond_area
            } else {
                0.0
            }
        })
        .collect();
    let mats = MaterialMap::new()
        .with(TAG_COND, ConstantLaw::vacuum())
        .with(TAG_GAP, ConstantLaw::vacuum())
        .with(TAG_IRON, brauer())
        .with(TAG_AIR, ConstantLaw::vacuum());
    let bsys = NonlinearMagnetostaticSystem::new(
        &mesh,
        &tags,
        &mats,
        &j,
        &disk_boundary_nodes(&mesh, R_OUT),
    )
    .unwrap();
    let bsol = bsys
        .solve(&NewtonConfig::default(), None)
        .expect("Brauer Newton converges");
    let b = recover_b_field(&mesh, &bsol.a_z);
    let b_iron_max = b
        .iter()
        .zip(&tags)
        .filter(|(_, g)| **g == TAG_IRON)
        .map(|(v, _)| (v[0] * v[0] + v[1] * v[1]).sqrt())
        .fold(0.0_f64, f64::max);
    println!("    Brauer iron: max |B| = {b_iron_max:.3} T");
    let bterm = newton_report("Newton, Brauer", &bsol);
    let p_brauer = assert_quadratic("Newton, Brauer", &bterm);

    // Frozen-ν fixed point on the table case from the same zero start.
    let fp_cfg = NewtonConfig {
        tangent: TangentKind::FrozenNu,
        max_iterations: 400,
        ..NewtonConfig::default()
    };
    let fp = sys.solve(&fp_cfg, None);
    let fp_hist = match &fp {
        Ok(s) => s.residual_norms.clone(),
        Err(NonlinearMagnetostaticError::NotConverged { residual_norms, .. }) => {
            residual_norms.clone()
        }
        Err(e) => panic!("frozen-nu iteration failed unexpectedly: {e}"),
    };
    let fp0 = fp_hist[0];
    println!(
        "    frozen-nu fixed point (same case, same config): {} iterations, {}",
        fp_hist.len() - 1,
        if fp.is_ok() {
            "converged"
        } else {
            "NOT converged at the cap"
        }
    );
    println!("      k | ||R_k||/||R_0|| | r_k/r_(k-1)");
    for (k, &r) in fp_hist.iter().enumerate() {
        if k < 6 || k % 10 == 0 || k + 1 == fp_hist.len() {
            let ratio = if k == 0 { f64::NAN } else { r / fp_hist[k - 1] };
            println!("      {k:3} | {:14.3e} | {ratio:.4}", r / fp0);
        }
    }
    // Linear convergence over the last 40 iterations: a geometric mean
    // ratio well inside (0, 1) and a fitted order of about 1.
    let w = &fp_hist[fp_hist.len() - 41..];
    let mean_ratio = (w[40] / w[0]).powf(1.0 / 40.0);
    let p_fp = loglog_order(w);
    println!(
        "    frozen-nu, last 40 iterations: geometric-mean ratio {mean_ratio:.4}, fitted order {p_fp:.3}; \
         Newton (table) {} iterations, fitted terminal order {p_table:.2}; Newton (Brauer) {} iterations, {p_brauer:.2}",
        sol.iterations, bsol.iterations
    );
    assert!(
        fp.is_ok(),
        "frozen-nu iteration should converge (linearly) within 400 iterations"
    );
    assert!(
        mean_ratio > 0.5 && mean_ratio < 0.99,
        "frozen-nu ratio {mean_ratio} is not linear convergence"
    );
    assert!(
        (0.8..1.2).contains(&p_fp),
        "frozen-nu fitted order {p_fp} should be ~1"
    );
    assert!(
        fp_hist.len() - 1 > 5 * sol.iterations,
        "frozen-nu should need far more iterations than Newton"
    );
}

/// The converged tangent drives the sensitivity `dA/dI = K⁻¹ F/I`
/// (`∂R/∂I = −F/I`). Compare it with a central FD of two converged solves,
/// as a preview of the M3 adjoint, and check the transpose solve.
#[test]
fn converged_tangent_gives_solution_sensitivity() {
    println!("\n[3b] converged tangent: dA/dI vs FD of converged solves (coax, n_ang=32)");
    let (mesh, tags) = coax_mesh(32);
    let i0 = coax_current();
    let cfg = NewtonConfig {
        rel_tol: 1e-10,
        ..NewtonConfig::default()
    };
    let sys = coax_system(&mesh, &tags, i0);
    let sol = sys.solve(&cfg, None).unwrap();
    let f_free = sys.restrict(sys.load());
    // Symmetric tangent: Kᵀ solve = K solve.
    let rhs: Vec<f64> = f_free.iter().map(|f| f / i0).collect();
    let da = sol.solve_tangent_transpose(&rhs).unwrap();
    let dh = 1e-3 * i0;
    let ap = sys_solve(&mesh, &tags, i0 + dh, &cfg);
    let am = sys_solve(&mesh, &tags, i0 - dh, &cfg);
    let fd: Vec<f64> = ap
        .iter()
        .zip(&am)
        .map(|(p, m)| (p - m) / (2.0 * dh))
        .collect();
    let fd_free = sys.restrict(&fd);
    let err = norm(
        &fd_free
            .iter()
            .zip(&da)
            .map(|(a, b)| a - b)
            .collect::<Vec<f64>>(),
    ) / norm(&da);
    println!("    ||FD dA/dI - K^-T F/I|| / ||K^-T F/I|| = {err:.3e} (dI/I = 1e-3)");
    assert!(err < 1e-6, "sensitivity mismatch {err:e}");
}

fn sys_solve(mesh: &TriMesh, tags: &[i32], i: f64, cfg: &NewtonConfig) -> Vec<f64> {
    coax_system(mesh, tags, i).solve(cfg, None).unwrap().a_z
}

// ─────────────────────────────────────────────────────────────────────
// 4. Manufactured solution
// ─────────────────────────────────────────────────────────────────────

const MMS_A: f64 = 0.35;

/// `A* = a e^y sin(2x + 1/2)` with gradient and Hessian.
fn mms_exact(x: f64, y: f64) -> (f64, [f64; 2], [[f64; 2]; 2]) {
    let (s, c) = (2.0 * x + 0.5).sin_cos();
    let e = MMS_A * y.exp();
    let val = e * s;
    let g = [2.0 * e * c, e * s];
    let h = [[-4.0 * e * s, 2.0 * e * c], [2.0 * e * c, e * s]];
    (val, g, h)
}

/// `f = −∇·(ν(|∇A*|²)∇A*) = −ν ΔA* − 2ν' (∇A*)ᵀ H (∇A*)`.
fn mms_source(law: &BrauerLaw, x: f64, y: f64) -> f64 {
    let (_v, g, h) = mms_exact(x, y);
    let b2 = g[0] * g[0] + g[1] * g[1];
    let lap = h[0][0] + h[1][1];
    let ghg = g[0] * (h[0][0] * g[0] + h[0][1] * g[1]) + g[1] * (h[1][0] * g[0] + h[1][1] * g[1]);
    -law.nu(b2) * lap - 2.0 * law.dnu_db2(b2) * ghg
}

fn fit_rate(hs: &[f64], es: &[f64]) -> f64 {
    let n = hs.len() as f64;
    let lx: Vec<f64> = hs.iter().map(|h| h.ln()).collect();
    let ly: Vec<f64> = es.iter().map(|e| e.ln()).collect();
    let mx = lx.iter().sum::<f64>() / n;
    let my = ly.iter().sum::<f64>() / n;
    let num: f64 = lx.iter().zip(&ly).map(|(x, y)| (x - mx) * (y - my)).sum();
    let den: f64 = lx.iter().map(|x| (x - mx).powi(2)).sum();
    num / den
}

#[test]
fn manufactured_solution_converges_at_optimal_rates() {
    println!("\n[4] manufactured solution, Brauer nu, A* = 0.35 e^y sin(2x+0.5) on [0,1]^2");
    let law = brauer();
    let levels = [8usize, 16, 32, 64, 128];
    let mut hs: Vec<f64> = Vec::new();
    let mut e_h1: Vec<f64> = Vec::new();
    let mut e_l2: Vec<f64> = Vec::new();
    println!(
        "      n |     h     | |A-A*|_H1 semi | rate | ||A-A*||_L2 | rate | Newton its | nu range (m/H)"
    );
    for &n in &levels {
        let mesh = rect_tri_mesh(n, n, 1.0, 1.0);
        let tags = vec![0; mesh.n_tris()];
        let mats = MaterialMap::new().with(0, law);
        let tol = 1e-12;
        let bc: Vec<bool> = mesh
            .nodes
            .iter()
            .map(|p| p[0] < tol || p[1] < tol || p[0] > 1.0 - tol || p[1] > 1.0 - tol)
            .collect();
        let g: Vec<f64> = mesh.nodes.iter().map(|p| mms_exact(p[0], p[1]).0).collect();
        let f = load_vector_from_fn(&mesh, |x, y| mms_source(&law, x, y));
        let sys =
            NonlinearMagnetostaticSystem::new(&mesh, &tags, &mats, &vec![0.0; mesh.n_tris()], &bc)
                .unwrap()
                .with_extra_load(&f)
                .unwrap()
                .with_dirichlet_values(&g)
                .unwrap();
        let start = sys.linear_start().unwrap();
        let sol = sys
            .solve(&NewtonConfig::default(), Some(&start))
            .expect("MMS Newton");
        let nu = sys.element_reluctivity(&sol.a_z);
        let (lo, hi) = nu
            .iter()
            .fold((f64::INFINITY, 0.0_f64), |(l, h), &x| (l.min(x), h.max(x)));

        let mut h1 = 0.0;
        let mut l2 = 0.0;
        for tri in &mesh.tris {
            let c = [
                mesh.nodes[tri[0] as usize],
                mesh.nodes[tri[1] as usize],
                mesh.nodes[tri[2] as usize],
            ];
            let det = (c[1][0] - c[0][0]) * (c[2][1] - c[0][1])
                - (c[2][0] - c[0][0]) * (c[1][1] - c[0][1]);
            let area = 0.5 * det.abs();
            let grads = [
                [(c[1][1] - c[2][1]) / det, (c[2][0] - c[1][0]) / det],
                [(c[2][1] - c[0][1]) / det, (c[0][0] - c[2][0]) / det],
                [(c[0][1] - c[1][1]) / det, (c[1][0] - c[0][0]) / det],
            ];
            let u = [
                sol.a_z[tri[0] as usize],
                sol.a_z[tri[1] as usize],
                sol.a_z[tri[2] as usize],
            ];
            let gh = [
                u[0] * grads[0][0] + u[1] * grads[1][0] + u[2] * grads[2][0],
                u[0] * grads[0][1] + u[1] * grads[1][1] + u[2] * grads[2][1],
            ];
            for q in TRI_QUAD_DEG4.iter() {
                let lam = [q[0], q[1], q[2]];
                let x = lam[0] * c[0][0] + lam[1] * c[1][0] + lam[2] * c[2][0];
                let y = lam[0] * c[0][1] + lam[1] * c[1][1] + lam[2] * c[2][1];
                let (ve, ge, _) = mms_exact(x, y);
                let uh = lam[0] * u[0] + lam[1] * u[1] + lam[2] * u[2];
                let w = q[3] * area;
                l2 += w * (uh - ve).powi(2);
                h1 += w * ((gh[0] - ge[0]).powi(2) + (gh[1] - ge[1]).powi(2));
            }
        }
        let (h1, l2) = (h1.sqrt(), l2.sqrt());
        let h = 1.0 / n as f64;
        let (r1, r2) =
            if let (Some(&hp), Some(&e1p), Some(&e2p)) = (hs.last(), e_h1.last(), e_l2.last()) {
                let lr = (hp / h).ln();
                ((e1p / h1).ln() / lr, (e2p / l2).ln() / lr)
            } else {
                (f64::NAN, f64::NAN)
            };
        println!(
            "    {n:3} | {h:.3e} | {h1:14.4e} | {r1:4.2} | {l2:11.4e} | {r2:4.2} | {:10} | {lo:.3e} .. {hi:.3e}",
            sol.iterations
        );
        hs.push(h);
        e_h1.push(h1);
        e_l2.push(l2);
    }
    let rate_h1 = fit_rate(&hs, &e_h1);
    let rate_l2 = fit_rate(&hs, &e_l2);
    println!(
        "    least-squares fitted rates over {} levels: H1-semi {rate_h1:.3}, L2 {rate_l2:.3}",
        levels.len()
    );
    assert!(
        (0.95..=1.1).contains(&rate_h1),
        "H1 seminorm rate {rate_h1:.3} not ~1"
    );
    assert!(
        (1.9..=2.2).contains(&rate_l2),
        "L2 rate {rate_l2:.3} not ~2"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 5. Saturated-iron Ampère oracle
// ─────────────────────────────────────────────────────────────────────

struct OracleRow {
    n_ang: usize,
    n_nodes: usize,
    iters: usize,
    residual_ls_its: usize,
    max_rel: f64,
    rms_rel: f64,
    mid_rel: f64,
    b_inner: f64,
    b_outer: f64,
}

fn ampere_oracle_level(n_ang: usize) -> OracleRow {
    let (mesh, tags) = coax_mesh(n_ang);
    let i = coax_current();
    let sys = coax_system(&mesh, &tags, i);
    let sol = sys
        .solve(&NewtonConfig::default(), None)
        .expect("coax Newton");
    let residual_ls_its = sys
        .solve(
            &NewtonConfig {
                line_search: LineSearch::ResidualNorm,
                ..NewtonConfig::default()
            },
            None,
        )
        .expect("coax Newton, residual-norm line search")
        .iterations;
    let b = recover_b_field(&mesh, &sol.a_z);
    let law = synthetic_steel();
    let r_mid = (R_IRON_IN * R_IRON_OUT).sqrt();
    let mut max_rel = 0.0_f64;
    let mut sum2 = 0.0;
    let mut cnt = 0usize;
    let mut mid_sum = 0.0;
    let mut mid_cnt = 0usize;
    let mut b_inner = 0.0_f64;
    let mut b_outer = f64::INFINITY;
    for (t, tri) in mesh.tris.iter().enumerate() {
        if tags[t] != TAG_IRON {
            continue;
        }
        let c = centroid(&mesh, tri);
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
        b_inner = b_inner.max(b_exact);
        b_outer = b_outer.min(b_exact);
    }
    OracleRow {
        n_ang,
        n_nodes: mesh.n_nodes(),
        iters: sol.iterations,
        residual_ls_its,
        max_rel,
        rms_rel: (sum2 / cnt as f64).sqrt(),
        mid_rel: mid_sum / mid_cnt as f64,
        b_inner,
        b_outer,
    }
}

#[test]
fn saturated_iron_ampere_oracle() {
    println!("\n[5] saturated-iron Ampere oracle: |B|(r) = B_of_H(I/(2 pi r)) in the iron annulus");
    println!(
        "    conductor r < {R_COND} m, iron {R_IRON_IN}..{R_IRON_OUT} m, Dirichlet ring at {R_OUT} m; \
         I = {:.2} A; mid-annulus = |ln(r/sqrt(r_i r_o))| < 0.1",
        coax_current()
    );
    println!(
        "    n_ang | nodes  | Newton its (energy LS / residual LS) | max rel err | RMS rel err | mid-annulus mean rel err | |B| range in iron (T)"
    );
    let mut rows = Vec::new();
    for n_ang in [32usize, 64, 128, 256] {
        let row = ampere_oracle_level(n_ang);
        println!(
            "    {:5} | {:6} | {:>17} / {:<17} | {:11.4e} | {:11.4e} | {:24.4e} | {:.3} .. {:.3}",
            row.n_ang,
            row.n_nodes,
            row.iters,
            row.residual_ls_its,
            row.max_rel,
            row.rms_rel,
            row.mid_rel,
            row.b_outer,
            row.b_inner
        );
        rows.push(row);
    }
    let first = &rows[0];
    let last = rows.last().unwrap();
    // Deep saturation at the inner radius, knee at the outer radius.
    assert!(
        first.b_inner > 1.8,
        "inner iron not deeply saturated: {}",
        first.b_inner
    );
    assert!(
        first.b_outer < 1.5,
        "outer iron not in the knee: {}",
        first.b_outer
    );
    // Converging with refinement.
    for w in rows.windows(2) {
        assert!(
            w[1].rms_rel < w[0].rms_rel,
            "RMS error not decreasing with refinement"
        );
    }
    assert!(
        last.mid_rel <= 0.01,
        "finest mid-annulus |B| error {:.4e} exceeds the 1% target",
        last.mid_rel
    );

    // Tripwire: the oracle must discriminate. A linear iron at the table's
    // initial permeability (no saturation) on the coarsest mesh misses the
    // saturated oracle by far more than the nonlinear solve does.
    let (mesh, tags) = coax_mesh(32);
    let i = coax_current();
    let mut j = vec![0.0; mesh.n_tris()];
    let cond_area: f64 = mesh
        .tris
        .iter()
        .zip(&tags)
        .filter(|(_, g)| **g == TAG_COND)
        .map(|(t, _)| tri_area(&mesh, t))
        .sum();
    for (jt, &g) in j.iter_mut().zip(&tags) {
        if g == TAG_COND {
            *jt = i / cond_area;
        }
    }
    let law = synthetic_steel();
    let mats = MaterialMap::new()
        .with(TAG_COND, ConstantLaw::vacuum())
        .with(TAG_GAP, ConstantLaw::vacuum())
        .with(
            TAG_IRON,
            ConstantLaw::new(law.initial_reluctivity()).unwrap(),
        )
        .with(TAG_AIR, ConstantLaw::vacuum());
    let lin = NonlinearMagnetostaticSystem::new(
        &mesh,
        &tags,
        &mats,
        &j,
        &disk_boundary_nodes(&mesh, R_OUT),
    )
    .unwrap()
    .solve(&NewtonConfig::default(), None)
    .unwrap();
    let b = recover_b_field(&mesh, &lin.a_z);
    let r_mid = (R_IRON_IN * R_IRON_OUT).sqrt();
    let (mut sum, mut cnt) = (0.0, 0usize);
    for (t, tri) in mesh.tris.iter().enumerate() {
        let c = centroid(&mesh, tri);
        let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
        if tags[t] == TAG_IRON && (r / r_mid).ln().abs() < 0.1 {
            let bh = (b[t][0] * b[t][0] + b[t][1] * b[t][1]).sqrt();
            sum += (bh - law.b_of_h(i / (TAU * r)).unwrap()).abs()
                / law.b_of_h(i / (TAU * r)).unwrap();
            cnt += 1;
        }
    }
    let trip = sum / cnt as f64;
    println!(
        "    tripwire: linear iron at the initial mu_r (n_ang=32) misses the mid-annulus oracle by {trip:.3e} \
         (nonlinear: {:.3e})",
        first.mid_rel
    );
    assert!(
        trip > 100.0 * first.mid_rel && trip > 0.1,
        "tripwire too weak: {trip:e}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 6. Law validation and loud failures
// ─────────────────────────────────────────────────────────────────────

fn expect_invalid(
    label: &str,
    r: Result<TabulatedBhLaw, NonlinearMagnetostaticError>,
    needle: &str,
) {
    match r {
        Err(NonlinearMagnetostaticError::InvalidLaw(msg)) => {
            println!("    {label:<34} -> rejected: {msg}");
            assert!(
                msg.contains(needle),
                "{label}: message {msg:?} lacks {needle:?}"
            );
        }
        other => panic!("{label}: expected InvalidLaw, got {other:?}"),
    }
}

#[test]
fn malformed_tables_and_parameters_are_rejected() {
    println!("\n[6] law validation");
    let h = SYNTH_H.to_vec();
    let b = SYNTH_B.to_vec();
    expect_invalid(
        "length mismatch",
        TabulatedBhLaw::from_hb(&h, &b[..10]),
        "points",
    );
    expect_invalid(
        "single point",
        TabulatedBhLaw::from_hb(&[0.0], &[0.0]),
        "at least 2",
    );
    let mut h_off = h.clone();
    h_off[0] = 5.0;
    expect_invalid(
        "does not start at origin",
        TabulatedBhLaw::from_hb(&h_off, &b),
        "origin",
    );
    let mut b_dup = b.clone();
    b_dup[5] = b_dup[4];
    expect_invalid(
        "repeated B",
        TabulatedBhLaw::from_hb(&h, &b_dup),
        "B must be strictly",
    );
    let mut h_dec = h.clone();
    h_dec[7] = h_dec[6] - 1.0;
    expect_invalid(
        "non-monotone H(B)",
        TabulatedBhLaw::from_hb(&h_dec, &b),
        "H must be strictly",
    );
    let mut b_nan = b.clone();
    b_nan[3] = f64::NAN;
    expect_invalid(
        "NaN entry",
        TabulatedBhLaw::from_hb(&h, &b_nan),
        "not finite",
    );
    // Stops at 1.58 T: last secant ≈ 1.9e4 ≪ 1/μ₀, no monotone C¹ tail join.
    expect_invalid(
        "table stops short of saturation",
        TabulatedBhLaw::from_hb(&h[..10], &b[..10]),
        "saturation",
    );
    // Brauer and constant-law parameter checks.
    for (k1, k2, k3) in [
        (-1.0, 2.0, 400.0),
        (3.8, -0.1, 400.0),
        (3.8, 2.0, 0.0),
        (f64::NAN, 2.0, 400.0),
    ] {
        let r = BrauerLaw::new(k1, k2, k3);
        println!("    Brauer({k1}, {k2}, {k3}) -> {:?}", r.as_ref().err());
        assert!(matches!(r, Err(NonlinearMagnetostaticError::InvalidLaw(_))));
    }
    assert!(ConstantLaw::new(0.0).is_err());
    assert!(ConstantLaw::new(f64::INFINITY).is_err());
    assert!(ConstantLaw::from_relative_permeability(-1.0).is_err());
}

#[test]
fn missing_material_and_non_convergence_are_errors() {
    let (mesh, tags) = coax_mesh(32);
    let j = vec![0.0; mesh.n_tris()];
    let bc = disk_boundary_nodes(&mesh, R_OUT);
    let partial = MaterialMap::new().with(TAG_COND, ConstantLaw::vacuum());
    let err = NonlinearMagnetostaticSystem::new(&mesh, &tags, &partial, &j, &bc).unwrap_err();
    assert!(
        matches!(err, NonlinearMagnetostaticError::MissingMaterial { .. }),
        "{err:?}"
    );

    // Deep saturation in 2 iterations cannot reach 1e-10: a returned error
    // carrying the history, never a silent last iterate.
    let sys = coax_system(&mesh, &tags, coax_current());
    let cfg = NewtonConfig {
        max_iterations: 2,
        ..NewtonConfig::default()
    };
    match sys.solve(&cfg, None) {
        Err(NonlinearMagnetostaticError::NotConverged {
            iterations,
            residual_norms,
            ..
        }) => {
            assert_eq!(iterations, 2);
            assert_eq!(residual_norms.len(), 3);
        }
        other => panic!("expected NotConverged, got {other:?}"),
    }
    // Residual-norm line search reaches the same converged state.
    let r_sol = sys
        .solve(
            &NewtonConfig {
                line_search: LineSearch::ResidualNorm,
                ..NewtonConfig::default()
            },
            None,
        )
        .unwrap();
    let e_sol = sys.solve(&NewtonConfig::default(), None).unwrap();
    let d = norm(
        &e_sol
            .a_z
            .iter()
            .zip(&r_sol.a_z)
            .map(|(a, b)| a - b)
            .collect::<Vec<f64>>(),
    ) / norm(&r_sol.a_z);
    println!(
        "\n[6b] energy (default) vs residual-norm line search: {} vs {} iterations, ||dA||/||A|| = {d:.2e}",
        e_sol.iterations, r_sol.iterations
    );
    assert!(d < 1e-9);
}

/// Warm starts and the round-off floor (Judge finding B1 on PR #1159). The
/// relative tolerance uses the guess-independent `reference_residual`, so:
/// (a) a restart from the converged solution returns at once; (b) a 0.1%
/// current change re-solved from the previous solution converges in a few
/// iterations. (c) Iterating past convergence with every stopping test off
/// shows where the residual stagnates; `roundoff_floor` must bound that
/// level from above, and not by more than 50x. (d) An unreachable
/// `rel_tol` ends as `RoundoffFloor` (success, with the achieved residual),
/// not as `LineSearchFailed`.
#[test]
fn warm_start_and_roundoff_floor() {
    println!("\n[7] warm starts and the round-off floor (coax, default config unless noted)");
    println!(
        "    n_ang | cold its | cold ||R||/ref | (a) restart its | (b) I*1.001 its, ||R||/ref | \
         (c) stagnation min ||R||/ref | floor est/ref | est/observed | (d) rel_tol 1e-14: status, its"
    );
    for n_ang in [64usize, 256] {
        let (mesh, tags) = coax_mesh(n_ang);
        let i0 = coax_current();
        let sys = coax_system(&mesh, &tags, i0);
        let reference = sys.reference_residual();
        let cold = sys
            .solve(&NewtonConfig::default(), None)
            .expect("cold solve");
        assert_eq!(cold.reference_residual, reference);

        // (a) Restart from the converged solution.
        let restart = sys
            .solve(&NewtonConfig::default(), Some(&cold.a_z))
            .expect("restart from the converged solution must succeed");
        assert!(
            restart.iterations <= 1,
            "restart took {} iterations",
            restart.iterations
        );

        // (b) 0.1% current change, warm-started from the previous solution.
        let sys_b = coax_system(&mesh, &tags, 1.001 * i0);
        let warm = sys_b
            .solve(&NewtonConfig::default(), Some(&cold.a_z))
            .expect("warm-started 0.1% current change must succeed");
        assert!(
            warm.iterations <= 5,
            "warm re-solve took {} iterations",
            warm.iterations
        );
        let cold_b = sys_b.solve(&NewtonConfig::default(), None).unwrap();
        let dev = norm(
            &warm
                .a_z
                .iter()
                .zip(&cold_b.a_z)
                .map(|(x, y)| x - y)
                .collect::<Vec<f64>>(),
        ) / norm(&cold_b.a_z);
        assert!(dev < 1e-8, "warm and cold re-solves differ by {dev:e}");

        // (c) Stagnation level with every stopping test disabled.
        let probe = NewtonConfig {
            rel_tol: 0.0,
            floor_factor: 0.0,
            max_iterations: 6,
            ..NewtonConfig::default()
        };
        let hist = match sys.solve(&probe, Some(&cold.a_z)) {
            Err(NonlinearMagnetostaticError::LineSearchFailed { residual_norms, .. })
            | Err(NonlinearMagnetostaticError::NotConverged { residual_norms, .. }) => {
                residual_norms
            }
            other => panic!("probe with all stopping tests off returned {other:?}"),
        };
        let observed = hist.iter().copied().fold(f64::INFINITY, f64::min);
        let est = sys.roundoff_floor(&cold.a_z);
        assert!(
            est >= observed && est <= 50.0 * observed,
            "floor estimate {est:e} does not bound the observed floor {observed:e} within 50x"
        );

        // (d) Unreachable tolerance: success at the floor, residual reported.
        let strict = sys
            .solve(
                &NewtonConfig {
                    rel_tol: 1e-14,
                    ..NewtonConfig::default()
                },
                None,
            )
            .expect("an unreachable rel_tol must end at the round-off floor, not fail");
        assert_eq!(strict.status, ConvergenceStatus::RoundoffFloor);
        let r_strict = *strict.residual_norms.last().unwrap();
        assert!(r_strict <= strict.floor_estimate);

        println!(
            "    {n_ang:5} | {:8} | {:14.3e} | {:15} | {:>9}, {:14.3e} | {:28.3e} | {:13.3e} | {:12.1} | {:?}, {}",
            cold.iterations,
            cold.residual_norms.last().unwrap() / reference,
            restart.iterations,
            warm.iterations,
            warm.residual_norms.last().unwrap() / sys_b.reference_residual(),
            observed / reference,
            est / reference,
            est / observed,
            strict.status,
            strict.iterations
        );
    }
}
