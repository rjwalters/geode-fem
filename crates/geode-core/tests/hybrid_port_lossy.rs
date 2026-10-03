//! Epic #778 Phase 4 (issue #806): **lossy and dispersive substrates** on
//! hybrid port faces — the complex-symmetric p=1 mixed pencil
//! (`analytic::lossy_port_modes`) and its use in the 3-D driven solve.
//!
//! Goldens:
//!
//! 1. **Complex roots** of the lossy slab-loaded guide (Newton continuation
//!    in `ε` from the lossless root): `Re β` ≤ 0.5 %, `α = −Im β` ≤ 2 %, at
//!    `tan δ ∈ {1e-3, 2e-2}`.
//! 2. **First-order α** from the lossless mode on the same mesh vs the exact
//!    lossy solve: ≤ 1 % at `tan δ = 1e-3`.
//! 3. **Microstrip α_d** (quasi-TEM, low frequency) vs Pozar: ≤ 3 %.
//! 4. **Dense oracle**: the complex Arnoldi and the dense complex
//!    shift-invert (#796) agree on every returned `β²` to ≤ 1e-8.
//! 5. **3-D lossy straight section**: `|S21| = e^{−αL}` ≤ 2 %, reciprocity
//!    `< 1e-8`, passivity `σ_max(S)` measured and reported.
//! 6. **Dispersive (Djordjevic–Sarkar) substrate** over ≥ 8 frequencies in
//!    the 3-D sweep with per-ω `ε`: port `β` vs the oracle at `ε(ω)`, phase
//!    and mode tracking continuous.
//! 7. **Lossless regression**: with `tan δ = 0` the complex path reproduces
//!    the real Phase 1 modes and the Phase 2 S-parameters to ≤ 1e-10.
//!
//! Run: `cargo test -p geode-core --release --test hybrid_port_lossy -- --nocapture`.

use faer::{Mat, c64};
use geode_core::analytic::loaded_guide::{LsFamily, SlabLoadedGuide};
use geode_core::analytic::lossy_port_modes::{
    LossyHybridModeSet, LossyRefinement, assemble_lossy_hybrid_pencil, first_order_beta_sq_shift,
    lossy_alpha_estimate, lossy_mode_accuracy, lossy_pairing, solve_lossy_hybrid_port_modes,
};
use geode_core::analytic::microstrip::{
    ShieldedStripFace, StripMeshOpts, hammerstad_jensen_eps_eff, pozar_dielectric_attenuation,
    square_coax_face,
};
use geode_core::analytic::port_modes::{
    HybridPortOpts, assemble_hybrid_blocks, discrete_gradient, solve_hybrid_port_modes,
};
use geode_core::analytic::waveguide::{
    TriMesh, rect_pec_interior_edges, rect_pec_interior_nodes, rect_tri_mesh,
};
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};

const A: f64 = 2.0;
const BH: f64 = 1.0;
const D: f64 = 0.5;

/// Structured slab face (`nx = 2·ny`), its masks, and the slab flags.
struct SlabFace {
    mesh: TriMesh,
    edge_mask: Vec<bool>,
    node_mask: Vec<bool>,
    in_slab: Vec<bool>,
}

fn slab_face(ny: usize) -> SlabFace {
    let mesh = rect_tri_mesh(2 * ny, ny, A, BH);
    let (_, edge_mask) = rect_pec_interior_edges(&mesh, A, BH);
    let node_mask = rect_pec_interior_nodes(&mesh, A, BH);
    let in_slab = mesh
        .tris
        .iter()
        .map(|t| t.iter().map(|&v| mesh.nodes[v as usize][1]).sum::<f64>() / 3.0 < D)
        .collect();
    SlabFace {
        mesh,
        edge_mask,
        node_mask,
        in_slab,
    }
}

impl SlabFace {
    fn eps(&self, slab: c64) -> Vec<c64> {
        self.in_slab
            .iter()
            .map(|&s| if s { slab } else { c64::new(1.0, 0.0) })
            .collect()
    }

    fn solve(&self, slab: c64, k0: f64, n_ev: usize) -> LossyHybridModeSet {
        solve_lossy_hybrid_port_modes(
            &self.mesh,
            &self.eps(slab),
            &self.edge_mask,
            &self.node_mask,
            k0,
            &HybridPortOpts {
                n_evanescent: n_ev,
                ..Default::default()
            },
        )
        .unwrap_or_else(|e| panic!("lossy solve at k0 = {k0}, ε = {slab}: {e}"))
    }
}

fn lossy(eps: f64, tan_d: f64) -> c64 {
    c64::new(eps, -eps * tan_d)
}

/// Lossless oracle modes (descending β²) and their continued complex roots.
fn oracle(eps: f64, target: c64, k0: f64, floor: f64) -> Vec<(LsFamily, u32, u32, f64, c64)> {
    let g = SlabLoadedGuide::new(A, BH, D, eps);
    g.modes(k0, floor)
        .into_iter()
        .map(|md| {
            let r = g
                .continued_root(md.family, md.m, md.n, k0, target, floor, 64)
                .expect("continued root");
            (md.family, md.m, md.n, md.beta_sq, r)
        })
        .collect()
}

fn outgoing(beta_sq: c64) -> c64 {
    let b = beta_sq.sqrt();
    if b.im > 0.0 { -b } else { b }
}

// ---------------------------------------------------------------------------
// 1. Complex roots of the lossy slab guide
// ---------------------------------------------------------------------------

/// **Golden 1.** FEM complex `β` of every propagating mode (`|β| ≥ 0.2 k₀`,
/// the Phase 1 filter) of the lossy slab guide vs the continued complex
/// root: `Re β` ≤ 0.5 %, `α` ≤ 2 % relative, asserted at h = b/32, with the
/// observed convergence rate of both errors over b/16 → b/32 (≥ 1.5).
///
/// Why b/32 and not the Phase 1 b/16: at b/16 the E_z-dominated LSE₁₁
/// (ε = 2.25, k₀ = 3) is 0.76 % off in `Re β`. That is the *lossless*
/// P1-carried-mode negative of #809 (the P1 Laplacian constant), not a loss
/// effect; the b/16 numbers are printed. Every mode converges at O(h²) in
/// both `Re β` and `α`.
#[test]
fn lossy_slab_complex_roots() {
    let faces = [slab_face(16), slab_face(32)];
    let mut worst = (0.0_f64, 0.0_f64);
    let mut min_rate = (f64::INFINITY, f64::INFINITY);
    let mut n_checked = 0usize;
    println!(
        "| ε | k₀ | tan δ | mode | Re β | α | Re err b/16 | α err b/16 | Re err b/32 | α err b/32 |"
    );
    for &(eps, k0) in &[(2.25, 2.0), (2.25, 3.0), (4.0, 1.5), (4.0, 3.0)] {
        for &tan_d in &[1e-3, 2e-2] {
            let target = lossy(eps, tan_d);
            let orc = oracle(eps, target, k0, 0.0);
            let n_prop = orc.iter().filter(|o| o.4.re > 0.0).count();
            let sets: Vec<LossyHybridModeSet> =
                faces.iter().map(|f| f.solve(target, k0, 2)).collect();
            for set in &sets {
                assert_eq!(set.n_propagating, n_prop, "ε {eps} k0 {k0} tanδ {tan_d}");
                assert!(
                    set.diagnostics.max_im_beta_sq_rel <= 2.0,
                    "the |Im β²| allowance of the certificate: {}",
                    set.diagnostics.max_im_beta_sq_rel
                );
            }
            for (i, o) in orc.iter().take(n_prop).enumerate() {
                let want = outgoing(o.4);
                if want.norm() < 0.2 * k0 {
                    continue;
                }
                let errs: Vec<(f64, f64)> = sets
                    .iter()
                    .map(|set| {
                        let md = &set.modes[i];
                        (
                            (md.beta.re - want.re).abs() / want.re,
                            (md.alpha() - (-want.im)).abs() / (-want.im),
                        )
                    })
                    .collect();
                println!(
                    "| {eps} | {k0} | {tan_d:.0e} | {:?}{}{} | {:.6} | {:.4e} | {:.2e} | {:.2e} | \
                     {:.2e} | {:.2e} |",
                    o.0, o.1, o.2, want.re, -want.im, errs[0].0, errs[0].1, errs[1].0, errs[1].1
                );
                let (re_err, a_err) = errs[1];
                assert!(re_err <= 5e-3, "Re β error {re_err:e} at b/32");
                assert!(a_err <= 2e-2, "α error {a_err:e} at b/32");
                let rate = |a: f64, b: f64| (a / b).log2();
                let r = (rate(errs[0].0, errs[1].0), rate(errs[0].1, errs[1].1));
                assert!(r.0 >= 1.5 && r.1 >= 1.5, "rates {r:?}");
                min_rate = (min_rate.0.min(r.0), min_rate.1.min(r.1));
                worst = (worst.0.max(re_err), worst.1.max(a_err));
                n_checked += 1;
            }
        }
    }
    println!(
        "golden 1: {n_checked} modes; worst at b/32: Re β {:.2e}, α {:.2e}; min rates {:.2} / {:.2}",
        worst.0, worst.1, min_rate.0, min_rate.1
    );
    assert!(n_checked >= 10);
}

// ---------------------------------------------------------------------------
// 2. First-order α (mesh-consistent)
// ---------------------------------------------------------------------------

/// **Golden 2.** At `tan δ = 1e-3` the exact lossy FEM `α` equals the
/// first-order `α` evaluated from the **lossless** FEM mode on the same
/// mesh (`first_order_beta_sq_shift`, module docs of `lossy_port_modes`) to
/// 1 %: a mesh-consistent check of the complex pencil, independent of the
/// oracle.
#[test]
fn first_order_alpha_matches_the_lossy_solve() {
    let face = slab_face(16);
    let tan_d = 1e-3;
    let mut worst = 0.0_f64;
    for &(eps, k0) in &[(2.25, 2.0), (2.25, 3.0), (4.0, 3.0)] {
        let lossless = face.solve(c64::new(eps, 0.0), k0, 0);
        let lossy_set = face.solve(lossy(eps, tan_d), k0, 0);
        assert_eq!(lossless.n_propagating, lossy_set.n_propagating);
        let delta: Vec<c64> = face
            .in_slab
            .iter()
            .map(|&s| {
                if s {
                    c64::new(0.0, -eps * tan_d)
                } else {
                    c64::new(0.0, 0.0)
                }
            })
            .collect();
        for (m0, m1) in lossless
            .modes
            .iter()
            .zip(&lossy_set.modes)
            .take(lossless.n_propagating)
        {
            let dbs = first_order_beta_sq_shift(&face.mesh, m0, &delta, k0);
            let alpha_pert = -(dbs / (2.0 * m0.beta)).im;
            let rel = (m1.alpha() - alpha_pert).abs() / alpha_pert;
            println!(
                "ε {eps} k0 {k0}: β₀ = {:.6} α (lossy solve) = {:.6e} α (first order) = \
                 {alpha_pert:.6e} rel {rel:.2e}; Re δβ² = {:.2e}",
                m0.beta.re,
                m1.alpha(),
                dbs.re
            );
            assert!(rel <= 1e-2, "first-order α off by {rel:e}");
            worst = worst.max(rel);
        }
    }
    println!("golden 2: worst {worst:.2e}");
}

// ---------------------------------------------------------------------------
// 3. Microstrip α_d vs Pozar
// ---------------------------------------------------------------------------

/// **Golden 3.** Shielded microstrip (`ε_r = 4.4`, `w/h = 1`, 20h box, the
/// Phase 3 fixture) at `f·h = 0.5 GHz·mm`, `tan δ = 0.02`: the quasi-TEM
/// mode's `α` vs Pozar's `α_d = k₀ε_r(ε_eff − 1)tan δ/(2√ε_eff(ε_r − 1))`,
/// with `ε_eff` from the lossless solve on the same face, ≤ 3 %. Also
/// printed against the Hammerstad–Jensen `ε_eff`.
#[test]
fn microstrip_dielectric_attenuation_vs_pozar() {
    let (wb, er, u, tan_d) = (20.0, 4.4, 1.0, 0.02);
    let face = ShieldedStripFace::microstrip(wb, wb, 1.0, u, er);
    let opts = StripMeshOpts {
        h_min: 0.02,
        h_max: 0.5,
        ratio: 1.25,
        mirror_symmetric: true,
    };
    let lossless_face = face.build(&opts);
    let fh = 0.5;
    let k0 = 2.0 * std::f64::consts::PI * fh / 299.792_458;
    let p1 = HybridPortOpts {
        n_evanescent: 0,
        ..Default::default()
    };
    let real = solve_hybrid_port_modes(
        &lossless_face.mesh,
        &lossless_face.eps_r,
        &lossless_face.masks.interior_edge_mask,
        &lossless_face.masks.free_node_mask,
        k0,
        &p1,
    )
    .expect("lossless microstrip");
    let eeff = real.modes[0].beta_sq / (k0 * k0);
    let eps_c: Vec<c64> = lossless_face
        .eps_r
        .iter()
        .map(|&e| {
            if e > 1.0 {
                lossy(e, tan_d)
            } else {
                c64::new(e, 0.0)
            }
        })
        .collect();
    let set = solve_lossy_hybrid_port_modes(
        &lossless_face.mesh,
        &eps_c,
        &lossless_face.masks.interior_edge_mask,
        &lossless_face.masks.free_node_mask,
        k0,
        &p1,
    )
    .expect("lossy microstrip");
    assert_eq!(set.n_propagating, 1);
    let m = &set.modes[0];
    let pozar = pozar_dielectric_attenuation(k0, er, eeff, tan_d);
    let ehj = hammerstad_jensen_eps_eff(u, er);
    let pozar_hj = pozar_dielectric_attenuation(k0, er, ehj, tan_d);
    let rel = m.alpha() / pozar - 1.0;
    println!(
        "microstrip f·h = {fh} GHz·mm: ε_eff = {eeff:.5} (HJ {ehj:.5}); α = {:.6e} Np/mm, Pozar \
         (FEM ε_eff) {pozar:.6e} ({:+.2}%), Pozar (HJ ε_eff) {pozar_hj:.6e} ({:+.2}%); \
         Re(β²)/k₀² = {:.5}; residual {:.1e} (floor {:.1e})",
        m.alpha(),
        100.0 * rel,
        100.0 * (m.alpha() / pozar_hj - 1.0),
        m.beta_sq.re / (k0 * k0),
        m.residual,
        m.residual_floor
    );
    assert!(rel.abs() <= 0.03, "α vs Pozar: {rel:+e}");
}

// ---------------------------------------------------------------------------
// 4. Dense oracle
// ---------------------------------------------------------------------------

/// **Golden 4.** Small lossy face (h = b/6, a few hundred DOFs): every
/// `β²` returned by the complex Arnoldi equals a dense complex
/// shift-invert eigenvalue (#796) of the same pencil to ≤ 1e-8 relative,
/// one-to-one, and the dense spectrum holds no eigenvalue in the returned
/// window that the Arnoldi missed.
#[test]
fn complex_arnoldi_matches_the_dense_oracle() {
    let face = slab_face(6);
    for &(eps, k0, tan_d) in &[(2.25, 2.0, 2e-2), (4.0, 3.0, 2e-2), (4.0, 3.0, 0.1)] {
        let slab = lossy(eps, tan_d);
        let set = face.solve(slab, k0, 4);
        let pencil = assemble_lossy_hybrid_pencil(
            &face.mesh,
            &face.eps(slab),
            &face.edge_mask,
            &face.node_mask,
            k0,
            true,
        )
        .unwrap();
        let n = pencil.free.len();
        let dense = |m: faer::sparse::SparseColMatRef<'_, usize, c64>| {
            let mut out = Mat::<c64>::zeros(n, n);
            let (cp, ri, v) = (m.col_ptr(), m.row_idx(), m.val());
            for j in 0..n {
                for p in cp[j]..cp[j + 1] {
                    out[(ri[p], j)] += v[p];
                }
            }
            out
        };
        let (da, db) = (dense(pencil.a.as_ref()), dense(pencil.b.as_ref()));
        let pairs = FaerComplexEigensolver
            .smallest_complex_pencil_pairs(da.as_ref(), db.as_ref(), n)
            .expect("dense oracle");
        let scale = k0 * k0 * eps;
        // Physical dense eigenvalues (drop the β² = 0 null cluster).
        let mut dense_bs: Vec<c64> = pairs
            .iter()
            .map(|(l, _)| -*l)
            .filter(|b| b.norm() > 1e-6 * scale)
            .collect();
        let n_null = pairs.len() - dense_bs.len();
        dense_bs.sort_by(|p, q| q.re.total_cmp(&p.re));
        let floor = set.modes.last().unwrap().beta_sq.re;
        let in_window: Vec<c64> = dense_bs
            .iter()
            .copied()
            .filter(|b| b.re >= floor - 1e-9 * scale)
            .collect();
        println!(
            "ε {eps} k0 {k0} tanδ {tan_d}: dim {n}, dense null cluster {n_null}, window {} \
             (Arnoldi {})",
            in_window.len(),
            set.modes.len()
        );
        assert_eq!(in_window.len(), set.modes.len(), "one-to-one in the window");
        let mut worst = 0.0_f64;
        for (md, d) in set.modes.iter().zip(&in_window) {
            let rel = (md.beta_sq - d).norm() / d.norm();
            worst = worst.max(rel);
            assert!(rel <= 1e-8, "β² {} vs dense {d}: {rel:e}", md.beta_sq);
        }
        println!("  worst |Δβ²|/|β²| = {worst:.2e}");
        assert_eq!(n_null, face.node_mask.iter().filter(|&&f| f).count());
    }
}

// ---------------------------------------------------------------------------
// Null space with complex ε
// ---------------------------------------------------------------------------

/// The exact `β² = 0` null space survives complex `ε`: without deflation
/// the null Ritz vectors sit at round-off in `|β²|` and in transverse
/// fraction, and the undeflated physical set equals the deflated one.
#[test]
fn null_space_stays_at_zero_with_complex_eps() {
    let face = slab_face(8);
    let (eps, k0) = (4.0, 2.5);
    let slab = lossy(eps, 0.05);
    let mk = |deflate: bool| {
        solve_lossy_hybrid_port_modes(
            &face.mesh,
            &face.eps(slab),
            &face.edge_mask,
            &face.node_mask,
            k0,
            &HybridPortOpts {
                n_evanescent: 3,
                deflate_null: deflate,
                ..Default::default()
            },
        )
        .unwrap()
    };
    let (on, off) = (mk(true), mk(false));
    println!(
        "deflated: null {} ; undeflated: null {} max |β²|/k₀²ε {:.2e} max η {:.2e}",
        on.diagnostics.null_vectors,
        off.diagnostics.null_vectors,
        off.diagnostics.max_null_beta_sq_rel,
        off.diagnostics.max_null_transverse_fraction
    );
    assert_eq!(on.diagnostics.null_vectors, 0);
    assert!(
        off.diagnostics.null_vectors >= 1,
        "undeflated run sees the null space"
    );
    assert!(off.diagnostics.max_null_beta_sq_rel <= 1e-12);
    assert!(off.diagnostics.max_null_transverse_fraction <= 1e-20);
    assert_eq!(on.modes.len(), off.modes.len());
    for (a, b) in on.modes.iter().zip(&off.modes) {
        assert!((a.beta_sq - b.beta_sq).norm() <= 1e-9 * k0 * k0 * eps);
    }
}

// ---------------------------------------------------------------------------
// 7. Lossless regression (2-D)
// ---------------------------------------------------------------------------

/// **Golden 7 (2-D).** With `Im ε = 0` the complex solver reproduces the
/// real Phase 1 solver: `β²` to ≤ 1e-10 relative and every field equal to
/// `pairing_scale ×` the real field (up to sign) to ≤ 1e-10, propagating and
/// evanescent modes alike.
#[test]
fn lossless_limit_reproduces_the_real_solver() {
    let face = slab_face(16);
    let mut worst = (0.0_f64, 0.0_f64);
    for &(eps, k0) in &[(2.25, 2.0), (4.0, 1.5), (4.0, 3.0)] {
        let real = solve_hybrid_port_modes(
            &face.mesh,
            &face
                .in_slab
                .iter()
                .map(|&s| if s { eps } else { 1.0 })
                .collect::<Vec<_>>(),
            &face.edge_mask,
            &face.node_mask,
            k0,
            &HybridPortOpts {
                n_evanescent: 3,
                ..Default::default()
            },
        )
        .unwrap();
        let cx = face.solve(c64::new(eps, 0.0), k0, 3);
        assert_eq!(real.modes.len(), cx.modes.len());
        assert_eq!(real.n_propagating, cx.n_propagating);
        for (r, c) in real.modes.iter().zip(&cx.modes) {
            let db = (c.beta_sq - r.beta_sq).norm() / r.beta_sq.abs();
            assert!(db <= 1e-10, "β² {} vs {}: {db:e}", c.beta_sq, r.beta_sq);
            assert!((c.beta - r.beta).norm() <= 1e-10 * r.beta.norm());
            let sc = r.pairing_scale();
            let nrm: f64 = r.e_t.iter().map(|v| v * v).sum::<f64>().sqrt();
            let diff = |sign: f64| {
                r.e_t
                    .iter()
                    .zip(&c.e_t)
                    .map(|(&x, &z)| (z - sc * (sign * x)).norm_sqr())
                    .chain(
                        r.e_z
                            .iter()
                            .zip(&c.e_z)
                            .map(|(&x, &z)| (z - sc * (sign * x)).norm_sqr()),
                    )
                    .sum::<f64>()
                    .sqrt()
                    / nrm
            };
            let df = diff(1.0).min(diff(-1.0));
            assert!(df <= 1e-10, "field mismatch {df:e} at β² = {}", r.beta_sq);
            assert!((c.norm - c.beta_sq).norm() <= 1e-12 * c.beta_sq.norm());
            worst = (worst.0.max(db), worst.1.max(df));
        }
    }
    println!(
        "golden 7 (2-D): worst β² {:.2e}, worst field {:.2e}",
        worst.0, worst.1
    );
}

// ---------------------------------------------------------------------------
// Repeated eigenvalues with loss (the Phase 3 multiplicity pass, complex)
// ---------------------------------------------------------------------------

/// Two strips in a **homogeneous lossy** box: the two TEM modes are exactly
/// degenerate at the complex `β² = k₀²ε`. The complex multiplicity pass
/// returns both, certified, and the unconjugated cluster Gram–Schmidt leaves
/// them B-orthogonal; the C4v coax TE11-like pair likewise.
#[test]
fn repeated_lossy_eigenvalues_return_every_copy() {
    let eps = lossy(2.2, 0.02);
    let mut two = ShieldedStripFace::microstrip(10.0, 6.0, 1.0, 1.0, 2.2);
    two.strips = vec![[-2.0, -1.0], [1.0, 2.0]];
    two.eps_above = 2.2;
    let f = two.build(&StripMeshOpts {
        h_min: 0.02,
        h_max: 0.5,
        ratio: 1.25,
        mirror_symmetric: false,
    });
    let k0 = 0.3;
    let eps_c = vec![eps; f.mesh.n_tris()];
    let set = solve_lossy_hybrid_port_modes(
        &f.mesh,
        &eps_c,
        &f.masks.interior_edge_mask,
        &f.masks.free_node_mask,
        k0,
        &HybridPortOpts {
            n_evanescent: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let tem: Vec<usize> = (0..set.modes.len())
        .filter(|&i| (set.modes[i].beta_sq / (eps * (k0 * k0)) - 1.0).norm() <= 1e-9)
        .collect();
    let m1 = assemble_hybrid_blocks(&f.mesh, &vec![2.2; f.mesh.n_tris()])
        .unwrap()
        .m1;
    let d = discrete_gradient(&f.mesh);
    let (a, b) = (&set.modes[tem[0]], &set.modes[tem[1]]);
    let rel = lossy_pairing(&m1, &d, a, b).norm() / (a.norm.norm() * b.norm.norm()).sqrt();
    println!(
        "lossy two strips: TEM copies {} (added {}, certified {}, clusters {}), pairing {rel:.2e}",
        tem.len(),
        set.diagnostics.repeated_copies,
        set.diagnostics.multiplicity_certified,
        set.diagnostics.degenerate_clusters
    );
    assert_eq!(tem.len(), 2, "both TEM modes");
    assert!(set.diagnostics.multiplicity_certified);
    assert!(rel <= 1e-10, "copies are B-orthogonal: {rel:e}");

    let c = square_coax_face(1.0, 12, 4, 2.2);
    let set = solve_lossy_hybrid_port_modes(
        &c.mesh,
        &vec![eps; c.mesh.n_tris()],
        &c.masks.interior_edge_mask,
        &c.masks.free_node_mask,
        2.0,
        &HybridPortOpts {
            n_evanescent: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let m1 = assemble_hybrid_blocks(&c.mesh, &vec![2.2; c.mesh.n_tris()])
        .unwrap()
        .m1;
    let d = discrete_gradient(&c.mesh);
    let mut n_pairs = 0;
    for i in 0..set.modes.len() - 1 {
        let (x, y) = (&set.modes[i], &set.modes[i + 1]);
        if (x.beta_sq - y.beta_sq).norm() <= 1e-10 * x.beta_sq.norm() {
            n_pairs += 1;
            let rel = lossy_pairing(&m1, &d, x, y).norm() / (x.norm.norm() * y.norm.norm()).sqrt();
            assert!(rel <= 1e-10, "C4v pair B-orthogonal: {rel:e}");
        }
    }
    println!(
        "lossy C4v coax: {n_pairs} degenerate pair(s), certified {}",
        set.diagnostics.multiplicity_certified
    );
    assert!(n_pairs >= 1 && set.diagnostics.multiplicity_certified);
}

// ---------------------------------------------------------------------------
// Accuracy estimate for complex modes (operator decision 2 of #804)
// ---------------------------------------------------------------------------

/// The per-mode `h/2` estimate on **lossy** modes tracks the true error
/// against the continued oracle root: the `β` estimate and the `α`
/// estimate ([`lossy_alpha_estimate`]) both within 2× (the Phase 2 bar),
/// at h = b/8 and b/16 over every propagating mode.
#[test]
fn lossy_accuracy_estimate_tracks_the_true_error() {
    let mut worst = 1.0_f64;
    let mut worst_a = 1.0_f64;
    let mut n = 0usize;
    for ny in [8usize, 16] {
        let face = slab_face(ny);
        for &(eps, k0) in &[(2.25, 2.0), (2.25, 3.0), (4.0, 1.5), (4.0, 3.0)] {
            let target = lossy(eps, 0.02);
            let set = face.solve(target, k0, 2);
            let orc = oracle(eps, target, k0, 0.0);
            let r = LossyRefinement::new(
                &face.mesh,
                &face.eps(target),
                &face.edge_mask,
                &face.node_mask,
            );
            let fine = r
                .solve(
                    k0,
                    &HybridPortOpts {
                        n_evanescent: 2,
                        max_krylov: 600,
                        ..Default::default()
                    },
                )
                .unwrap();
            let coarse: Vec<_> = set.modes.iter().take(set.n_propagating).collect();
            let acc = lossy_mode_accuracy(&r, &coarse, &fine, None);
            for ((md, o), a) in coarse.iter().zip(&orc).zip(&acc) {
                let want = outgoing(o.4);
                if want.norm() < 0.2 * k0 {
                    continue;
                }
                let a = a.expect("matched refined mode");
                let true_err = (md.beta - want).norm() / want.norm();
                let f = (a.estimate / true_err).max(true_err / a.estimate);
                let true_a = (md.alpha() + want.im).abs() / (-want.im);
                let est_a = lossy_alpha_estimate(&a).unwrap();
                let fa = (est_a / true_a).max(true_a / est_a);
                println!(
                    "b/{ny} ε {eps} k0 {k0} {:?}{}{}: β true {true_err:.2e} est {:.2e} (×{f:.2}); \
                     α true {true_a:.2e} est {est_a:.2e} (×{fa:.2})",
                    o.0, o.1, o.2, a.estimate
                );
                assert!(f <= 2.0, "β estimate off by {f}×");
                assert!(fa <= 2.0, "α estimate off by {fa}×");
                worst = worst.max(f);
                worst_a = worst_a.max(fa);
                n += 1;
            }
        }
    }
    println!("lossy accuracy: {n} modes, worst β factor {worst:.2}, worst α factor {worst_a:.2}");
}

// ===========================================================================
// 3-D: lossy / dispersive hybrid wave ports in the driven solve
// ===========================================================================

use burn::tensor::backend::BackendTypes;
use geode_core::driven::ports::{
    ExtrudedWaveguideMesh, HybridPortFace, HybridWavePort, HybridWavePortOpts, WavePortSpec,
    WavePortSpecSweep, extruded_rect_waveguide_mesh,
    solve_wave_port_spec_sweep_dispersive_with_mode, solve_wave_port_spec_sweep_with_mode,
};
use geode_core::driven::rom::{DrivenRom, RomError, RomSettings};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SolverMode,
};
use geode_core::testing::TestBackend;

type B = TestBackend;

const LEN: f64 = 1.2;
/// Single-mode band of the ε = 2.25 slab guide (LSE₀₁ cuts on at k₀ ≈ 2.42).
const BAND: [f64; 8] = [1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2];

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// The Phase 2 slab fixture: `16 × 8 × 4`, face h = b/8.
fn guide3d() -> ExtrudedWaveguideMesh {
    extruded_rect_waveguide_mesh(16, 8, 4, A, BH, LEN)
}

fn slab_tets(g: &ExtrudedWaveguideMesh) -> Vec<bool> {
    g.mesh
        .tets
        .iter()
        .map(|t| t.iter().map(|&v| g.mesh.nodes[v as usize][1]).sum::<f64>() / 4.0 < D)
        .collect()
}

fn eps3d(g: &ExtrudedWaveguideMesh, slab: c64) -> Vec<c64> {
    slab_tets(g)
        .into_iter()
        .map(|s| if s { slab } else { c64::new(1.0, 0.0) })
        .collect()
}

fn no_accuracy() -> HybridWavePortOpts {
    HybridWavePortOpts {
        accuracy: None,
        ..Default::default()
    }
}

fn lossy_ports(
    g: &ExtrudedWaveguideMesh,
    eps: &[c64],
    opts: HybridWavePortOpts,
) -> [WavePortSpec; 2] {
    let mk = |faces: &[[u32; 3]]| {
        WavePortSpec::from(
            HybridWavePort::new(
                HybridPortFace::from_volume_lossy(&g.mesh, faces, eps).expect("lossy face"),
                vec![c64::new(1.0, 0.0)],
            )
            .with_opts(opts),
        )
    };
    [mk(&g.port1_faces), mk(&g.port2_faces)]
}

fn try_sweep(
    g: &ExtrudedWaveguideMesh,
    eps: &[c64],
    ports: &[WavePortSpec],
    omegas: &[f64],
) -> Result<WavePortSpecSweep, DrivenError> {
    let pec = g.pec_interior_mask();
    solve_wave_port_spec_sweep_with_mode::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        ports,
        &[],
        omegas,
        SolverMode::Direct,
        &device(),
    )
}

/// Largest singular value of the `n × n` S (power iteration on `SᴴS`).
fn sigma_max(s: &[c64], n: usize) -> f64 {
    let mut v = vec![c64::new(1.0, 0.3); n];
    let mut lam = 0.0;
    for _ in 0..500 {
        let sv: Vec<c64> = (0..n)
            .map(|i| (0..n).fold(c64::new(0.0, 0.0), |a, j| a + s[i * n + j] * v[j]))
            .collect();
        let w: Vec<c64> = (0..n)
            .map(|j| (0..n).fold(c64::new(0.0, 0.0), |a, i| a + s[i * n + j].conj() * sv[i]))
            .collect();
        let nrm = w.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        lam = nrm / v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        v = w.iter().map(|z| z / nrm).collect();
    }
    lam.sqrt()
}

fn unwrap_phase(z: &[c64]) -> Vec<f64> {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut out: Vec<f64> = Vec::with_capacity(z.len());
    for v in z {
        let a = v.arg();
        let p = match out.last() {
            None => a,
            Some(&prev) => {
                let d = (a - prev + std::f64::consts::PI).rem_euclid(two_pi) - std::f64::consts::PI;
                prev + d
            }
        };
        out.push(p);
    }
    out
}

/// Continued LSM₁₀ root of the ε′ = `eps_re` slab at complex `target`.
fn lsm10(eps_re: f64, target: c64, k0: f64) -> c64 {
    outgoing(
        SlabLoadedGuide::new(A, BH, D, eps_re)
            .continued_root(LsFamily::Lsm, 1, 0, k0, target, 0.0, 64)
            .expect("LSM10 root"),
    )
}

// ---------------------------------------------------------------------------
// 5. Lossy straight section
// ---------------------------------------------------------------------------

/// **Golden 5.** Lossy slab straight section (`ε = 2.25(1 − j tan δ)`,
/// `tan δ ∈ {0.02, 0.1}`, the Phase 2 `16 × 8 × 4` fixture) over the
/// single-mode band: `|S21| = e^{−αL}` within 2 % (`α` from the continued
/// oracle root), port `β` vs the oracle (`Re` 0.5 %, `α` 2 %), reciprocity
/// `|S12 − S21| < 1e-8`, and `σ_max(S)` measured (asserted ≤ 1 + 1e-6 and
/// reported: with loss the pseudo-power normalization does not guarantee
/// it, curation question 2).
#[test]
fn lossy_straight_section() {
    let g = guide3d();
    println!(
        "| tan δ | k₀ | Re β | α | port α err | \\|S21\\| | e^(−αL) | rel | −ln\\|S21\\|/(αL) | \\|S11\\| | recip | σ_max |"
    );
    let mut worst_s21 = 0.0_f64;
    let mut worst_smax = 0.0_f64;
    for &tan_d in &[0.02, 0.1] {
        let slab = lossy(2.25, tan_d);
        let eps = eps3d(&g, slab);
        let ports = lossy_ports(&g, &eps, no_accuracy());
        let out = try_sweep(&g, &eps, &ports, &BAND).expect("lossy sweep");
        for p in &out.points {
            let want = lsm10(2.25, slab, p.omega);
            let alpha = -want.im;
            let beta = p.beta[0];
            let re_err = (beta.re - want.re).abs() / want.re;
            let a_err = (-beta.im - alpha).abs() / alpha;
            let (s11, s21, s12) = (p.s[0], p.s[2], p.s[1]);
            let att = (-alpha * LEN).exp();
            let rel = s21.norm() / att - 1.0;
            let recip = (s12 - s21).norm();
            let smax = sigma_max(&p.s, 2);
            println!(
                "| {tan_d} | {:.1} | {:.5} | {alpha:.4e} | {a_err:.2e} | {:.5} | {att:.5} | \
                 {rel:+.2e} | {:.4} | {:.2e} | {recip:.1e} | {smax:.6} |",
                p.omega,
                want.re,
                s21.norm(),
                -s21.norm().ln() / (alpha * LEN),
                s11.norm(),
            );
            assert!(re_err <= 5e-3, "port Re β {re_err:e}");
            assert!(a_err <= 2e-2, "port α {a_err:e}");
            assert!(rel.abs() <= 0.02, "|S21| vs e^(−αL): {rel:+e}");
            assert!(recip < 1e-8, "reciprocity {recip:e}");
            assert!(smax <= 1.0 + 1e-6, "σ_max {smax}");
            worst_s21 = worst_s21.max(rel.abs());
            worst_smax = worst_smax.max(smax);
        }
        for r in &out.hybrid {
            for pt in r.points.iter().skip(1) {
                assert!(pt.channels[0].track_overlap.unwrap() >= 0.9);
                assert!(pt.channels[0].beta_sq_im < 0.0, "passive: Im β² < 0");
            }
        }
    }
    println!("golden 5: worst ||S21|/e^(−αL) − 1| = {worst_s21:.2e}, max σ_max = {worst_smax:.8}");
}

// ---------------------------------------------------------------------------
// 6. Dispersive (Djordjevic–Sarkar) substrate, per-ω ε
// ---------------------------------------------------------------------------

/// Djordjevic–Sarkar `ε(ω)` (the `geode-cli` `dispersion::DjordjevicSarkar`
/// model, #757, in natural units: the band corners and the reference point
/// are angular wavenumbers, the ratio form is unit-free), fitted to
/// `ε(ω_ref) = ε′(1 − j tan δ)`.
struct Ds {
    w1: f64,
    w2: f64,
    eps_inf: f64,
    delta: f64,
}

impl Ds {
    fn fit(eps: f64, tan_d: f64, w_ref: f64, w1: f64, w2: f64) -> Self {
        let l = (w2 / w1).log10();
        let span = (w_ref / w1).atan() - (w_ref / w2).atan();
        let delta = eps * tan_d * l * std::f64::consts::LN_10 / span;
        let re_g = 0.5 * ((w2 * w2 + w_ref * w_ref) / (w1 * w1 + w_ref * w_ref)).log10();
        Self {
            w1,
            w2,
            eps_inf: eps - delta * re_g / l,
            delta,
        }
    }

    fn eps(&self, w: f64) -> c64 {
        let l = (self.w2 / self.w1).log10();
        let re_g = 0.5 * ((self.w2 * self.w2 + w * w) / (self.w1 * self.w1 + w * w)).log10();
        let im_g = ((w / self.w2).atan() - (w / self.w1).atan()) / std::f64::consts::LN_10;
        c64::new(self.eps_inf + self.delta * re_g / l, self.delta * im_g / l)
    }
}

/// **Golden 6.** A Djordjevic–Sarkar slab (`ε′ = 2.25`, `tan δ = 0.05` at
/// `k₀ = 1.85`, corners `10⁻²` … `10³`) swept over 8 frequencies through
/// the **dispersive** sweep, which re-assembles the volume and re-solves the
/// port faces from the same per-tet `ε(ω)`:
///
/// - port `β` vs the continued oracle root at the DS `ε(ω)` of each
///   frequency: `Re` ≤ 0.5 %, `α` ≤ 2 %;
/// - mode tracking continuous (overlap ≥ 0.9), the unwrapped `arg S21`
///   follows `−Re β L` (no sign flip or 2π jump), `|S21| = e^{−αL}` ≤ 2 %;
/// - reciprocity `< 1e-8`;
/// - the DS model really disperses over the band (`Re ε` and `Im ε` vary),
///   and a **fixed**-ε sweep at the reference value differs visibly at the
///   band edges (tripwire: the per-ω path is exercised).
#[test]
fn dispersive_djordjevic_sarkar_substrate() {
    let g = guide3d();
    let ds = Ds::fit(2.25, 0.05, 1.85, 1e-2, 1e3);
    let in_slab = slab_tets(&g);
    let eps_at = |w: f64| -> Vec<c64> {
        let e = ds.eps(w);
        in_slab
            .iter()
            .map(|&s| if s { e } else { c64::new(1.0, 0.0) })
            .collect()
    };
    // The face ε for the port construction (any frequency: the dispersive
    // sweep replaces it per ω through the tet map).
    let eps_ref = eps_at(1.85);
    // Default options: the per-mode accuracy estimate runs on the per-ω
    // refined faces too.
    let ports = lossy_ports(&g, &eps_ref, HybridWavePortOpts::default());
    let pec = g.pec_interior_mask();
    let out = solve_wave_port_spec_sweep_dispersive_with_mode::<B>(
        &g.mesh,
        &eps_at,
        None,
        &DrivenBcs {
            pec_interior_mask: &pec,
        },
        &ports,
        &[],
        &BAND,
        SolverMode::Direct,
        &device(),
    )
    .expect("dispersive sweep");
    let fixed = try_sweep(&g, &eps_ref, &ports, &BAND).expect("fixed sweep");
    let mut s21s = Vec::new();
    let mut want_phase = Vec::new();
    let (e_lo, e_hi) = (ds.eps(BAND[0]), ds.eps(BAND[7]));
    println!("DS ε at band edges: {e_lo} … {e_hi}");
    assert!((e_lo.re - e_hi.re).abs() > 1e-3 && (e_lo.im - e_hi.im).abs() > 1e-4);
    let mut max_fixed_diff = 0.0_f64;
    for (p, pf) in out.points.iter().zip(&fixed.points) {
        let e = ds.eps(p.omega);
        let want = lsm10(e.re, e, p.omega);
        let beta = p.beta[0];
        let re_err = (beta.re - want.re).abs() / want.re;
        let a_err = (-beta.im + want.im).abs() / (-want.im);
        let s21 = p.s[2];
        let att = (want.im * LEN).exp();
        let rel = s21.norm() / att - 1.0;
        let recip = (p.s[1] - s21).norm();
        let dfix = (s21 - pf.s[2]).norm();
        max_fixed_diff = max_fixed_diff.max(dfix);
        println!(
            "k0 {:.2}: ε = {:.5}{:+.5}j β = {:.6}{:+.4e}j (oracle {:.6}{:+.4e}j) Re err \
             {re_err:.2e} α err {a_err:.2e} |S21| {:.5} vs {att:.5} ({rel:+.2e}) recip {recip:.1e} \
             |ΔS21| vs fixed ε {dfix:.2e}",
            p.omega,
            e.re,
            e.im,
            beta.re,
            beta.im,
            want.re,
            want.im,
            s21.norm()
        );
        assert!(
            re_err <= 5e-3 && a_err <= 2e-2,
            "β vs oracle at the DS ε(ω)"
        );
        assert!(rel.abs() <= 0.02, "|S21| vs e^(−αL)");
        assert!(recip < 1e-8);
        s21s.push(s21);
        want_phase.push(-beta.re * LEN);
    }
    let ph = unwrap_phase(&s21s);
    for (i, (a, b)) in ph.iter().zip(&want_phase).enumerate() {
        assert!(
            (a - b).abs() < 0.05,
            "k0 #{i}: unwrapped arg S21 {a} vs −βL {b}"
        );
    }
    for r in &out.hybrid {
        for pt in r.points.iter().skip(1) {
            let ov = pt.channels[0].track_overlap.expect("tracked");
            assert!(ov >= 0.9, "port {} k0 {}: overlap {ov}", r.port, pt.omega);
        }
        for pt in &r.points {
            let ch = &pt.channels[0];
            let acc = ch.accuracy.expect("accuracy estimate");
            let a_est = ch.alpha_accuracy.expect("α estimate");
            println!(
                "port {} k0 {:.2}: β estimate {:.2e}, α estimate {a_est:.2e} (rate {:.2})",
                r.port, pt.omega, acc.estimate, acc.rate
            );
            assert!(acc.estimate < 5e-3 && a_est < 5e-2);
        }
    }
    for w in &out.warnings {
        println!("warning: {}", w.message);
    }
    assert!(out.warnings.is_empty(), "no accuracy warning on this face");
    println!("fixed-ε tripwire: max |ΔS21| = {max_fixed_diff:.3e}");
    assert!(
        max_fixed_diff > 1e-3,
        "the per-ω ε must change S at the band edges"
    );
}

// ---------------------------------------------------------------------------
// 7. Lossless regression (3-D) and the volume-consistency check
// ---------------------------------------------------------------------------

/// **Golden 7 (3-D).** The lossy path with `tan δ = 0` (a complex face with
/// `Im ε = 0`) reproduces the real Phase 2 slab straight section: S and `β`
/// to ≤ 1e-10 over the band, with the accuracy estimate on (estimates agree
/// to ≤ 1e-8).
#[test]
fn lossless_limit_reproduces_the_phase2_sweep() {
    let g = guide3d();
    let eps_c = eps3d(&g, c64::new(2.25, 0.0));
    let eps_r: Vec<f64> = eps_c.iter().map(|e| e.re).collect();
    let opts = HybridWavePortOpts::default();
    let real_ports: Vec<WavePortSpec> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| {
            WavePortSpec::from(
                HybridWavePort::new(
                    HybridPortFace::from_volume(&g.mesh, f, &eps_r).unwrap(),
                    vec![c64::new(1.0, 0.0)],
                )
                .with_opts(opts),
            )
        })
        .collect();
    let omegas = [1.5, 1.8, 2.1];
    let real = try_sweep(&g, &eps_c, &real_ports, &omegas).unwrap();
    let cx = try_sweep(&g, &eps_c, &lossy_ports(&g, &eps_c, opts), &omegas).unwrap();
    let (mut ws, mut wb, mut we) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (r, c) in real.points.iter().zip(&cx.points) {
        for (a, b) in r.s.iter().zip(&c.s) {
            ws = ws.max((a - b).norm());
        }
        wb = wb.max((r.beta[0] - c.beta[0]).norm() / r.beta[0].norm());
    }
    for (hr, hc) in real.hybrid.iter().zip(&cx.hybrid) {
        for (pr, pc) in hr.points.iter().zip(&hc.points) {
            let (ar, ac) = (
                pr.channels[0].accuracy.unwrap(),
                pc.channels[0].accuracy.unwrap(),
            );
            we = we.max((ar.estimate - ac.estimate).abs() / ar.estimate);
            // Im β² is round-off on a lossless face (~1e-20 measured).
            let ch = &pc.channels[0];
            assert!(
                ch.beta_sq_im.abs() <= 1e-12 * ch.beta_sq.abs(),
                "{}",
                ch.beta_sq_im
            );
        }
    }
    println!("golden 7 (3-D): max |ΔS| = {ws:.2e}, max |Δβ|/|β| = {wb:.2e}, accuracy {we:.2e}");
    assert!(ws <= 1e-10 && wb <= 1e-10, "lossless regression");
    assert!(we <= 1e-8, "accuracy estimates");
}

/// A hybrid face whose `ε` differs from the volume's — a lossless face on a
/// lossy volume, or a stale loss tangent — is a loud `InvalidPort`, not a
/// silently inconsistent port; the adaptive PROM rejects lossy hybrid ports
/// like every hybrid port (#774).
#[test]
fn face_volume_mismatch_is_loud_and_prom_rejects_lossy_ports() {
    let g = extruded_rect_waveguide_mesh(8, 4, 2, A, BH, LEN);
    let vol = eps3d(&g, lossy(2.25, 0.02));
    let stale = eps3d(&g, lossy(2.25, 0.021));
    let real: Vec<f64> = vol.iter().map(|e| e.re).collect();
    let real_ports: Vec<WavePortSpec> = [&g.port1_faces, &g.port2_faces]
        .iter()
        .map(|f| {
            WavePortSpec::from(HybridWavePort::new(
                HybridPortFace::from_volume(&g.mesh, f, &real).unwrap(),
                vec![c64::new(1.0, 0.0)],
            ))
        })
        .collect();
    for (what, ports) in [
        ("lossless face, lossy volume", real_ports.clone()),
        (
            "stale tan δ",
            lossy_ports(&g, &stale, no_accuracy()).to_vec(),
        ),
    ] {
        let err = try_sweep(&g, &vol, &ports, &[1.8]).expect_err(what);
        println!("{what}: {err}");
        assert!(
            matches!(&err, DrivenError::InvalidPort { reason, .. } if reason.contains("volume's own permittivity")),
            "{what}: {err}"
        );
    }
    // Gain is refused at construction.
    let gain = eps3d(&g, c64::new(2.25, 0.01));
    assert!(HybridPortFace::from_volume_lossy(&g.mesh, &g.port1_faces, &gain).is_err());

    let pec = g.pec_interior_mask();
    let bcs = DrivenBcs {
        pec_interior_mask: &pec,
    };
    let op = DrivenOperator::assemble::<B>(
        &g.mesh,
        DrivenMaterials::Scalar(&vol),
        None,
        &bcs,
        &[],
        &[],
        &CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; g.mesh.n_tets()],
        },
        &device(),
    )
    .unwrap();
    let err = DrivenRom::build_with_wave_port_specs(
        &op,
        &g.mesh,
        &bcs,
        &lossy_ports(&g, &vol, no_accuracy()),
        &[1.6, 1.8, 2.0],
        &RomSettings::default(),
        &mut |_| {},
    )
    .err()
    .expect("lossy hybrid ports must be rejected by the PROM");
    assert!(
        matches!(&err, RomError::InvalidParameter(m) if m.contains("not affine")),
        "{err}"
    );
}
