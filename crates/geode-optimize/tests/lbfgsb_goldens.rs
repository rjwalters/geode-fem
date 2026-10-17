//! L-BFGS-B goldens (issue #873): Rosenbrock 2-D / 10-D with and without
//! bounds, a bound-constrained quadratic with an exact active set, and the
//! strong Wolfe conditions along real search directions.
//!
//! The reference iteration counts are SciPy 1.17 `minimize(method="L-BFGS-B",
//! maxcor=10)` (the Fortran L-BFGS-B 3.0 code) from the same start. They are
//! printed for comparison and checked only loosely (within 3x), because the
//! line searches differ: SciPy uses Moré–Thuente `dcsrch`, while this crate
//! uses Nocedal–Wright bracketing and zoom.

mod common;

use common::{Rosenbrock, max_abs_diff, rosenbrock};
use geode_optimize::{
    Bounds, Convergence, FnObjective, Lbfgsb, LbfgsbOptions, LineSearchOptions, Objective,
    Optimizer, Status, StopCriteria, strong_wolfe,
};

fn opts(pg_tol: f64) -> LbfgsbOptions {
    LbfgsbOptions {
        stop: StopCriteria {
            pg_tol,
            f_rel_tol: 0.0,
            max_iter: 2000,
            max_evals: 5000,
            ..StopCriteria::default()
        },
        ..LbfgsbOptions::default()
    }
}

fn report(name: &str, st: &geode_optimize::LbfgsbState, scipy_iter: usize, scipy_nfev: usize) {
    let p = &st.progress;
    eprintln!(
        "| {name} | L-BFGS-B | {} | {} | {} / {} | {:.3e} | {:?} |",
        p.iter, p.n_evals, scipy_iter, scipy_nfev, p.f, p.status
    );
    assert!(
        p.iter <= 3 * scipy_iter.max(10),
        "{name}: {} iterations vs SciPy {scipy_iter}",
        p.iter
    );
}

#[test]
fn rosenbrock_2d_unbounded() {
    let mut obj = Rosenbrock::new(2);
    let st = Lbfgsb::new(opts(1e-10))
        .minimize(&mut obj, &[-1.2, 1.0], &Bounds::unbounded(2))
        .unwrap();
    assert_eq!(
        st.progress.status,
        Status::Converged {
            reason: Convergence::FirstOrder
        }
    );
    assert!(max_abs_diff(st.x(), &[1.0, 1.0]) < 1e-8, "{:?}", st.x());
    assert!(st.f() < 1e-16);
    assert_eq!(obj.calls, st.progress.n_evals);
    report("Rosenbrock 2-D", &st, 38, 46);
}

#[test]
fn rosenbrock_2d_with_active_bound() {
    // x₁ ≤ 0.5 is active: the optimum is (0.5, 0.25) with f = 0.25 exactly
    // (f ≥ (1 − x₁)² ≥ 0.25 on the box, with equality only there).
    let mut obj = Rosenbrock::new(2);
    let b = Bounds::new(vec![-2.0, -1.0], vec![0.5, 2.0]).unwrap();
    let st = Lbfgsb::new(opts(1e-10))
        .minimize(&mut obj, &[-1.2, 1.0], &b)
        .unwrap();
    assert!(
        st.progress.status.is_converged(),
        "{:?}",
        st.progress.status
    );
    assert_eq!(st.x()[0], 0.5, "the bound is hit exactly");
    assert!((st.x()[1] - 0.25).abs() < 1e-10);
    assert!((st.f() - 0.25).abs() < 1e-14);
    report("Rosenbrock 2-D, x₁ ≤ 0.5", &st, 20, 30);
}

#[test]
fn rosenbrock_10d_unbounded() {
    let n = 10;
    let x0: Vec<f64> = (0..n)
        .map(|i| if i % 2 == 0 { -1.2 } else { 1.0 })
        .collect();
    let mut obj = Rosenbrock::new(n);
    let st = Lbfgsb::new(opts(1e-9))
        .minimize(&mut obj, &x0, &Bounds::unbounded(n))
        .unwrap();
    assert!(
        st.progress.status.is_converged(),
        "{:?}",
        st.progress.status
    );
    assert!(max_abs_diff(st.x(), &vec![1.0; n]) < 1e-8, "{:?}", st.x());
    report("Rosenbrock 10-D", &st, 76, 94);
}

#[test]
fn rosenbrock_10d_in_a_box_with_eight_active_bounds() {
    // On [1.2, 3]¹⁰ seven variables sit at the lower bound and the last at
    // the upper bound. The KKT point is unique (20 random SciPy starts all
    // agree). Reference: SciPy L-BFGS-B, gtol = 1e-12.
    let n = 10;
    let reference = [
        1.2,
        1.2,
        1.2,
        1.2,
        1.2,
        1.2,
        1.2,
        1.331_928_296_765_808_6,
        1.734_709_391_808_687_8,
        3.0,
    ];
    let mut obj = Rosenbrock::new(n);
    let b = Bounds::uniform(n, 1.2, 3.0);
    let st = Lbfgsb::new(opts(1e-8))
        .minimize(&mut obj, &vec![2.0; n], &b)
        .unwrap();
    assert!(
        st.progress.status.is_converged(),
        "{:?}",
        st.progress.status
    );
    assert!(max_abs_diff(st.x(), &reference) < 1e-9, "{:?}", st.x());
    for (i, &want) in reference.iter().enumerate() {
        if want == 1.2 || want == 3.0 {
            assert_eq!(st.x()[i], want, "x[{i}] is exactly at its bound");
        }
    }
    assert!((st.f() - 36.821_052_816_198_45).abs() < 1e-9);
    report("Rosenbrock 10-D in [1.2, 3]¹⁰", &st, 18, 22);
}

/// `½xᵀAx − bᵀx` on a box, with `b` built so that a chosen x* is the KKT
/// point with strict complementarity (exact active set, unique since A ≻ 0).
#[test]
fn bound_constrained_quadratic_identifies_the_exact_active_set() {
    let n = 30;
    // A = tridiag(-1, 4, -1) + 0.5·(cos-coupled low-rank term): SPD.
    let mut a = vec![0.0; n * n];
    for i in 0..n {
        a[i * n + i] = 4.0;
        if i + 1 < n {
            a[i * n + i + 1] = -1.0;
            a[(i + 1) * n + i] = -1.0;
        }
    }
    let u: Vec<f64> = (0..n).map(|i| (i as f64 * 0.7).cos()).collect();
    for i in 0..n {
        for j in 0..n {
            a[i * n + j] += 0.5 * u[i] * u[j];
        }
    }
    let lower: Vec<f64> = (0..n).map(|i| -1.0 - 0.1 * (i % 3) as f64).collect();
    let upper: Vec<f64> = (0..n).map(|i| 1.0 + 0.05 * (i % 4) as f64).collect();
    // Active pattern: i % 5 == 0 → lower, i % 5 == 1 → upper, else free.
    let mut xstar = vec![0.0; n];
    let mut resid = vec![0.0; n]; // ∇f(x*) = A x* − b
    for i in 0..n {
        match i % 5 {
            0 => {
                xstar[i] = lower[i];
                resid[i] = 0.5 + 0.1 * i as f64; // > 0: pushes into the lower bound
            }
            1 => {
                xstar[i] = upper[i];
                resid[i] = -(0.3 + 0.05 * i as f64);
            }
            _ => xstar[i] = 0.4 * ((i as f64) * 1.3).sin(),
        }
    }
    let axs: Vec<f64> = (0..n)
        .map(|i| (0..n).map(|j| a[i * n + j] * xstar[j]).sum())
        .collect();
    let b: Vec<f64> = (0..n).map(|i| axs[i] - resid[i]).collect();
    let mut obj = FnObjective::new(n, move |x: &[f64]| {
        let ax: Vec<f64> = (0..n)
            .map(|i| (0..n).map(|j| a[i * n + j] * x[j]).sum())
            .collect();
        let f: f64 = (0..n).map(|i| 0.5 * x[i] * ax[i] - b[i] * x[i]).sum();
        Ok((f, (0..n).map(|i| ax[i] - b[i]).collect()))
    });
    let bounds = Bounds::new(lower.clone(), upper.clone()).unwrap();
    let x0 = vec![0.0; n];
    let st = Lbfgsb::new(opts(1e-10))
        .minimize(&mut obj, &x0, &bounds)
        .unwrap();
    assert!(
        st.progress.status.is_converged(),
        "{:?}",
        st.progress.status
    );
    for i in 0..n {
        match i % 5 {
            0 => assert_eq!(st.x()[i], lower[i], "x[{i}] must be at its lower bound"),
            1 => assert_eq!(st.x()[i], upper[i], "x[{i}] must be at its upper bound"),
            _ => assert!(
                st.x()[i] > lower[i] && st.x()[i] < upper[i],
                "x[{i}] must be free"
            ),
        }
    }
    let err = max_abs_diff(st.x(), &xstar);
    assert!(err < 1e-9, "‖x − x*‖∞ = {err:e}");
    eprintln!(
        "| Box QP n=30, 12 active | L-BFGS-B | {} | {} | — | ‖x−x*‖∞ = {err:.1e} | {:?} |",
        st.progress.iter, st.progress.n_evals, st.progress.status
    );
}

#[test]
fn accepted_steps_satisfy_strong_wolfe_along_rosenbrock_directions() {
    // Directions −∇f from several points; every accepted step must satisfy
    // sufficient decrease and the strong curvature condition, re-checked
    // here against an independent evaluation.
    let o = LineSearchOptions::default();
    for x in [
        vec![-1.2, 1.0],
        vec![0.0, 0.0],
        vec![2.0, -1.0],
        vec![0.9, 0.7],
        vec![-0.5, 2.5],
    ] {
        let (f0, g0) = rosenbrock(&x);
        let d: Vec<f64> = g0.iter().map(|g| -g).collect();
        let dphi0: f64 = g0.iter().zip(&d).map(|(a, b)| a * b).sum();
        let phi = |a: f64| {
            let xt: Vec<f64> = x.iter().zip(&d).map(|(x, d)| x + a * d).collect();
            let (f, g) = rosenbrock(&xt);
            (f, g.iter().zip(&d).map(|(a, b)| a * b).sum::<f64>())
        };
        let a0 = 1.0 / dphi0.abs().sqrt();
        let out = strong_wolfe(f0, dphi0, a0, f64::INFINITY, &o, |a| Ok(phi(a))).unwrap();
        let (fa, da) = phi(out.alpha);
        assert!(out.strong_wolfe, "{x:?}: {out:?}");
        assert!(
            fa <= f0 + o.c1 * out.alpha * dphi0,
            "{x:?}: sufficient decrease"
        );
        assert!(da.abs() <= o.c2 * dphi0.abs(), "{x:?}: curvature");
    }
}

#[test]
fn every_lbfgsb_iteration_decreases_f_and_stays_feasible() {
    let mut obj = Rosenbrock::new(10);
    let b = Bounds::uniform(10, 1.2, 3.0);
    let st = Lbfgsb::new(opts(1e-8))
        .minimize(&mut obj, &[2.0; 10], &b)
        .unwrap();
    let h = &st.progress.history;
    assert_eq!(h.len(), st.progress.iter + 1);
    for w in h.windows(2) {
        assert!(
            w[1].f <= w[0].f,
            "monotone decrease at iteration {}",
            w[1].iter
        );
        assert!(w[1].x.iter().all(|v| (1.2..=3.0).contains(v)));
    }
    assert_eq!(h.last().unwrap().n_evals, st.progress.n_evals);
}

#[test]
fn memory_size_and_stopping_criteria() {
    // m = 3 still converges (more iterations); max_iter / max_evals / f_rel
    // stop with the right statuses.
    let mut obj = Rosenbrock::new(10);
    let x0 = vec![-1.2, 1.0, -1.2, 1.0, -1.2, 1.0, -1.2, 1.0, -1.2, 1.0];
    let ub = Bounds::unbounded(10);
    let st = Lbfgsb::new(LbfgsbOptions {
        memory: 3,
        ..opts(1e-8)
    })
    .minimize(&mut obj, &x0, &ub)
    .unwrap();
    assert!(st.progress.status.is_converged());
    assert!(max_abs_diff(st.x(), &[1.0; 10]) < 1e-6);

    let mk = |stop: StopCriteria| {
        Lbfgsb::new(LbfgsbOptions {
            stop,
            ..LbfgsbOptions::default()
        })
    };
    let st = mk(StopCriteria {
        pg_tol: 0.0,
        f_rel_tol: 0.0,
        max_iter: 5,
        ..StopCriteria::default()
    })
    .minimize(&mut Rosenbrock::new(10), &x0, &ub)
    .unwrap();
    assert_eq!(st.progress.status, Status::MaxIterations);
    assert_eq!(st.progress.iter, 5);

    let st = mk(StopCriteria {
        pg_tol: 0.0,
        f_rel_tol: 0.0,
        max_evals: 12,
        ..StopCriteria::default()
    })
    .minimize(&mut Rosenbrock::new(10), &x0, &ub)
    .unwrap();
    assert_eq!(st.progress.status, Status::MaxEvaluations);
    assert!(st.progress.n_evals <= 12);

    let st = mk(StopCriteria {
        pg_tol: 0.0,
        f_rel_tol: 1e-3,
        ..StopCriteria::default()
    })
    .minimize(&mut Rosenbrock::new(10), &x0, &ub)
    .unwrap();
    assert_eq!(
        st.progress.status,
        Status::Converged {
            reason: Convergence::RelativeFChange
        }
    );
    // A start that is already optimal converges at iteration 0.
    let mut r = Rosenbrock::new(3);
    let st = Lbfgsb::default()
        .minimize(&mut r, &[1.0, 1.0, 1.0], &Bounds::unbounded(3))
        .unwrap();
    assert!(st.progress.status.is_converged());
    assert_eq!((st.progress.iter, r.calls), (0, 1));
    assert_eq!(r.dim(), 3);
}
