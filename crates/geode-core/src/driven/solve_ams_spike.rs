//! Issue #708 Phase 6b — **AMS-for-driven research spike** (measurement
//! harness only, `#[ignore]`d; no shipped code path depends on it).
//!
//! Question: can the eigen path's real three-space Hiptmair–Xu AMS
//! ([`crate::eigen::ams::AmsLitePreconditioner`], built for the
//! shift-invert inner solve) precondition the complex-symmetric driven
//! operator `A(ω) = K − ω²M(ε) + Σ (iω/Z_s) S_Γ + Σ (iω/Z_p) S_p` under
//! COCG well enough to converge where Jacobi / ILU(0) stall (the spiral,
//! #676)?
//!
//! Approach (the curated #708 recommendation): build the real AMS once per
//! ω on a **real proxy** `P` of `A(ω)` and apply it to the real and
//! imaginary parts of each COCG residual independently (a real symmetric
//! operator applied to a complex vector is complex-symmetric, so it
//! composes with the COCG bilinear form). Two proxies:
//!
//! - `spd`: `P = K + ω² Re M(ε) + Σ Re(iω/Z_s) S_Γ + Σ |ω/Z_p| S_p` — the
//!   sign-flipped SPD proxy of the eigen path (#559 / #607).
//! - `phys`: `P = K − ω² Re M(ε) + Σ Re(iω/Z_s) S_Γ` — the physical-sign
//!   real part (indefinite on the gradient block only, where the nodal
//!   coarse operator `Gᵀ P G` is then negative definite).
//!
//! Run: `cargo test -p geode-core --release --lib ams_spike -- --ignored
//! --nocapture` (`GEODE_SPIKE_MESH=benchmark` for the 54k-edge mesh).

use super::*;
use crate::eigen::ams::{AmsLitePreconditioner, CoarseSolve};
use crate::eigen::projection::InteriorGradient;
use crate::mesh::spiral::CONDUCTOR_SIGMA_NATURAL;
use crate::mesh::{
    pec_interior_mask_from_triangles, read_spiral_fixture, read_spiral_smoke_fixture,
};
use crate::solver::ksp::{Cocg, KspSolve, Preconditioner};
use crate::testing::TestBackend;
use burn::tensor::backend::BackendTypes;
use faer::sparse::{SparseColMat, Triplet};

type B = TestBackend;

/// Real CSC SpMV `y = P x`.
fn spmv(p: &SparseColMat<usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let pr = p.as_ref();
    let (cp, ri, va) = (pr.col_ptr(), pr.row_idx(), pr.val());
    for j in 0..pr.ncols() {
        let xj = x[j];
        for k in cp[j]..cp[j + 1] {
            y[ri[k]] += va[k] * xj;
        }
    }
}

/// Real AMS applied to Re / Im independently (V-cycle against the proxy).
struct SplitAms {
    ams: AmsLitePreconditioner,
    proxy: SparseColMat<usize, f64>,
    n: usize,
    vcycle: bool,
}

impl Preconditioner for SplitAms {
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        let re: Vec<f64> = r.iter().map(|c| c.re).collect();
        let im: Vec<f64> = r.iter().map(|c| c.im).collect();
        let mut zr = vec![0.0; self.n];
        let mut zi = vec![0.0; self.n];
        if self.vcycle {
            self.ams
                .apply_vcycle(&re, &mut zr, |x, y| spmv(&self.proxy, x, y));
            self.ams
                .apply_vcycle(&im, &mut zi, |x, y| spmv(&self.proxy, x, y));
        } else {
            self.ams.apply(&re, &mut zr);
            self.ams.apply(&im, &mut zi);
        }
        for i in 0..self.n {
            z[i] = c64::new(zr[i], zi[i]);
        }
    }

    fn dim(&self) -> usize {
        self.n
    }
}

/// Build the real proxy `P` (see module docs) from the operator's cached
/// ω-independent pieces.
fn proxy(op: &DrivenOperator, omega: f64, spd: bool) -> SparseColMat<usize, f64> {
    let w2 = omega * omega;
    let sign = if spd { 1.0 } else { -1.0 };
    let coeffs: Vec<c64> = op
        .surfaces
        .iter()
        .map(|s| s.model.weak_coefficient(omega).expect("Z_s"))
        .collect();
    let mut t: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(op.rows.len());
    for idx in 0..op.rows.len() {
        let mut v = op.k_vals[idx].re + sign * w2 * op.m_vals[idx].re;
        for (s, c) in op.surfaces.iter().zip(coeffs.iter()) {
            v += c.re * s.s_vals[idx];
        }
        t.push(Triplet::new(op.rows[idx], op.cols[idx], v));
    }
    if spd {
        for port in &op.ports {
            let scale = (omega / port.z_s).abs();
            for &(r, c, v) in &port.mass_triplets {
                t.push(Triplet::new(r, c, scale * v));
            }
        }
    }
    SparseColMat::try_new_from_triplets(op.n_interior, op.n_interior, &t).expect("proxy")
}

#[test]
#[ignore = "issue #708 6b AMS-for-driven research spike (measurement harness)"]
fn ams_spike_spiral() {
    let bench = std::env::var("GEODE_SPIKE_MESH").as_deref() == Ok("benchmark");
    let fixture = if bench {
        read_spiral_fixture()
    } else {
        read_spiral_smoke_fixture()
    }
    .expect("spiral fixture");
    let device = <B as BackendTypes>::Device::default();
    let mesh = &fixture.mesh;
    let edges = mesh.edges();
    let eps = fixture.epsilon_r_default();
    let outer = fixture.outer_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer.as_slice()]);
    let cond = fixture.conductor_triangles();
    let surface = SurfaceImpedanceBc {
        triangles: &cond,
        model: SurfaceImpedanceModel::GoodConductor {
            sigma: CONDUCTOR_SIGMA_NATURAL,
        },
    };
    let port = fixture.port();
    let lp = port.lumped_port(50.0 / crate::constants::ETA_0_OHM, c64::new(1.0, 0.0));
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&lp),
        std::slice::from_ref(&surface),
        &source,
        &device,
    )
    .expect("assemble");
    let n = op.n_interior;
    let interior_index: Vec<Option<usize>> = op
        .remap
        .iter()
        .map(|&r| if r >= 0 { Some(r as usize) } else { None })
        .collect();
    let mut edge_vectors = vec![[0.0_f64; 3]; n];
    for (e, &row) in interior_index.iter().enumerate() {
        if let Some(row) = row {
            let [a, b] = edges[e];
            let pa = mesh.nodes[a as usize];
            let pb = mesh.nodes[b as usize];
            edge_vectors[row] = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        }
    }
    let gradient = InteriorGradient::build(&edges, &mask, &interior_index, mesh.n_nodes(), n)
        .with_edge_vectors(edge_vectors);
    eprintln!(
        "mesh={} n_interior={n} node_dim={}",
        if bench { "benchmark" } else { "smoke" },
        gradient.node_dim()
    );

    let budget: usize = std::env::var("GEODE_SPIKE_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    let ksp = Cocg::new(1e-10, budget);
    let freqs: Vec<f64> = if bench {
        vec![1.0]
    } else {
        vec![1.0, 5.0, 10.0, 20.0]
    };
    for f_ghz in freqs {
        let omega = 2.0 * std::f64::consts::PI * f_ghz * 1e9 / crate::constants::C_UM_PER_S;
        let a = op.assemble_a_at(omega).expect("A(ω)");
        let b = op.assemble_b_at(omega, None);
        // Direct-LU reference solution and the true-residual check, so a
        // "converged" Krylov report is verified against A and LU, not
        // just the COCG recurrence residual.
        let x_lu: Vec<c64> = {
            use faer::linalg::solvers::Solve;
            let lu = a.as_ref().sp_lu().expect("LU");
            let mut rhs = faer::Mat::<c64>::zeros(n, 1);
            for (i, &bi) in b.iter().enumerate() {
                rhs[(i, 0)] = bi;
            }
            lu.solve_in_place(rhs.as_mut());
            (0..n).map(|i| rhs[(i, 0)]).collect()
        };
        let norm = |v: &[c64]| v.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
        let checks = |x: &[c64]| {
            let mut ax = vec![c64::new(0.0, 0.0); n];
            let ar = a.as_ref();
            for (j, &xj) in x.iter().enumerate() {
                for k in ar.col_ptr()[j]..ar.col_ptr()[j + 1] {
                    ax[ar.row_idx()[k]] += ar.val()[k] * xj;
                }
            }
            let r: Vec<c64> = b.iter().zip(&ax).map(|(bi, ai)| bi - ai).collect();
            let d: Vec<c64> = x.iter().zip(&x_lu).map(|(xi, li)| xi - li).collect();
            (norm(&r) / norm(&b), norm(&d) / norm(&x_lu))
        };
        let report = |name: &str, p: &SolveFn<'_>| {
            let mut x = vec![c64::new(0.0, 0.0); n];
            let t0 = std::time::Instant::now();
            match p(&b, &mut x) {
                Ok(r) => {
                    let dt = t0.elapsed().as_secs_f64();
                    let (true_res, err_lu) = checks(&x);
                    eprintln!(
                        "  f={f_ghz:>4} GHz {name:<18} stopped converged={} iters={:>5} res={:.2e} \
                             true_res={true_res:.2e} vs_lu={err_lu:.2e} t={dt:.2}s",
                        r.converged, r.iters, r.res
                    )
                }
                Err(e) => eprintln!(
                    "  f={f_ghz:>4} GHz {name:<18} FAILED {e} t={:.2}s",
                    t0.elapsed().as_secs_f64()
                ),
            }
        };
        let run = |pc: &dyn PrecondDyn, b: &[c64], x: &mut [c64]| {
            ksp.solve(a.as_ref(), b, x, &DynWrap(pc))
                .map(|r| KspReportLite {
                    iters: r.iters,
                    res: r.residual_rel,
                    converged: r.converged,
                })
                .map_err(|e| e.to_string())
        };
        let jac = IterativePreconditioner::Jacobi
            .build(a.as_ref())
            .expect("jacobi");
        report("jacobi", &|b, x| run(&jac, b, x));
        match IterativePreconditioner::Ilu0.build(a.as_ref()) {
            Ok(ilu) => report("ilu0", &|b, x| run(&ilu, b, x)),
            Err(e) => eprintln!("  ilu0 setup failed: {e}"),
        }
        for (spd, vcycle, coarse, label) in [
            (true, true, CoarseSolve::default(), "ams-spd-vc-sgs"),
            (true, true, CoarseSolve::Direct, "ams-spd-vc-lu"),
            (true, true, CoarseSolve::Amg, "ams-spd-vc-amg"),
        ] {
            let t_setup = std::time::Instant::now();
            let p = proxy(&op, omega, spd);
            let ams = match AmsLitePreconditioner::build_with_coarse(
                &gradient,
                p.as_ref(),
                p.as_ref(),
                0.0,
                coarse,
            ) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("  {label} setup failed: {e}");
                    continue;
                }
            };
            eprintln!("  {label} setup={:.2}s", t_setup.elapsed().as_secs_f64());
            let pc = SplitAms {
                ams,
                proxy: p,
                n,
                vcycle,
            };
            report(label, &|b, x| run(&pc, b, x));
        }
    }
}

/// A boxed-up "run one preconditioned COCG solve" closure.
type SolveFn<'a> = dyn Fn(&[c64], &mut [c64]) -> Result<KspReportLite, String> + 'a;

struct KspReportLite {
    iters: usize,
    res: f64,
    converged: bool,
}

/// Object-safe shim so the harness can loop over heterogeneous
/// preconditioners.
trait PrecondDyn {
    fn apply_dyn(&self, r: &[c64], z: &mut [c64]);
    fn dim_dyn(&self) -> usize;
}

impl<P: Preconditioner> PrecondDyn for P {
    fn apply_dyn(&self, r: &[c64], z: &mut [c64]) {
        self.apply(r, z)
    }
    fn dim_dyn(&self) -> usize {
        self.dim()
    }
}

struct DynWrap<'a>(&'a dyn PrecondDyn);

impl Preconditioner for DynWrap<'_> {
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        self.0.apply_dyn(r, z)
    }
    fn dim(&self) -> usize {
        self.0.dim_dyn()
    }
}
