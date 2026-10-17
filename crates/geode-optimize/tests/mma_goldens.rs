//! MMA goldens (issue #873): Hock–Schittkowski inequality problems (HS71
//! with its equality relaxed, HS76, HS21), Svanberg's 1987 cantilever and
//! two-bar truss, and a minimum-compliance problem with a closed-form
//! optimum. Each one runs in plain MMA and in GCMMA (`globalize = true`).
//!
//! The published optima are from Hock & Schittkowski, *Test Examples for
//! Nonlinear Programming Codes* (1981), and Svanberg (1987). They were
//! cross-checked with SciPy SLSQP at `ftol = 1e-15`. The SciPy iteration
//! counts are printed for orientation only: SLSQP is an SQP method, so its
//! counts are not comparable with MMA's.

mod common;

use common::{Cantilever, Compliance, Hs21, Hs71, Hs76, TwoBarTruss, max_abs_diff};
use geode_optimize::{Bounds, Mma, MmaOptions, MmaState, Objective, Optimizer, StopCriteria};

fn opts(globalize: bool) -> MmaOptions {
    MmaOptions {
        stop: StopCriteria {
            pg_tol: 1e-7,
            f_rel_tol: 0.0,
            feas_tol: 1e-9,
            max_iter: 500,
            max_evals: 5000,
            ..StopCriteria::default()
        },
        globalize,
        ..MmaOptions::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    name: &str,
    obj: &mut dyn Objective,
    x0: &[f64],
    b: &Bounds,
    globalize: bool,
    f_star: f64,
    x_star: &[f64],
    tol_x: f64,
) -> MmaState {
    let st = Mma::new(opts(globalize)).minimize(obj, x0, b).unwrap();
    let p = &st.progress;
    let label = if globalize { "GCMMA" } else { "MMA" };
    eprintln!(
        "| {name} | {label} | {} | {} | f = {:.10} (published {f_star}) | max g = {:.1e} | {:?} |",
        p.iter,
        p.n_evals,
        p.f,
        p.max_violation(),
        p.status
    );
    assert!(p.status.is_converged(), "{name} {label}: {:?}", p.status);
    assert!(p.max_violation() <= 1e-9, "{name} {label}: infeasible");
    let rel = (p.f - f_star).abs() / f_star.abs().max(1.0);
    assert!(
        rel < 1e-7,
        "{name} {label}: f = {} vs {f_star} (rel {rel:e})",
        p.f
    );
    let dx = max_abs_diff(&p.x, x_star);
    assert!(
        dx < tol_x,
        "{name} {label}: x = {:?}, ‖x − x*‖∞ = {dx:e}",
        p.x
    );
    st
}

#[test]
fn hs71_relaxed() {
    // f* = 17.0140173 at (1, 4.7429994, 3.8211503, 1.3794082); start (1, 5, 5, 1).
    let xs = [1.0, 4.742_999_63, 3.821_150_00, 1.379_408_29];
    for g in [false, true] {
        run(
            "HS71 (≤ form)",
            &mut Hs71,
            &[1.0, 5.0, 5.0, 1.0],
            &Bounds::uniform(4, 1.0, 5.0),
            g,
            17.014_017_289,
            &xs,
            1e-5,
        );
    }
}

#[test]
fn hs76() {
    // f* = −4.681818181 at (0.2727273, 2.090909, 0, 0.5454545).
    let xs = [3.0 / 11.0, 23.0 / 11.0, 0.0, 6.0 / 11.0];
    for g in [false, true] {
        run(
            "HS76",
            &mut Hs76,
            &[0.5; 4],
            &Bounds::uniform(4, 0.0, 10.0),
            g,
            -4.681_818_181_818_182,
            &xs,
            1e-6,
        );
    }
}

/// HS21 is the honest negative: **plain MMA does not converge on it**. With
/// `x₂ ∈ [−50, 50]` the asymptotes cannot contract below
/// `0.01·(xmax − xmin) = 1` from `x` (Svanberg's `asymin`). The separable
/// approximation of `x₂²` is then nearly one-sided, so `x₂` falls into a
/// stable 2-cycle between the move limits around 0. That is the textbook
/// reason GCMMA exists. GCMMA's conservative inner loop raises `ρ` until the
/// approximation dominates `x₂²`, and it reaches f* = −99.96 at (2, 0). The
/// test pins both behaviours, so a change to either one is noticed.
#[test]
fn hs21_plain_mma_cycles_and_gcmma_converges() {
    let b = Bounds::new(vec![2.0, -50.0], vec![50.0, 50.0]).unwrap();
    let plain = Mma::new(MmaOptions {
        stop: StopCriteria {
            max_iter: 200,
            ..opts(false).stop
        },
        ..opts(false)
    })
    .minimize(&mut Hs21, &[-1.0, -1.0], &b)
    .unwrap();
    let h = &plain.progress.history;
    eprintln!(
        "| HS21 | MMA | {} | {} | f = {:.10} (published -99.96) | 2-cycle x₂ ∈ {{{:.4}, {:.4}}} | {:?} |",
        plain.progress.iter,
        plain.progress.n_evals,
        plain.progress.f,
        h[h.len() - 1].x[1],
        h[h.len() - 2].x[1],
        plain.progress.status
    );
    assert_eq!(plain.progress.status, geode_optimize::Status::MaxIterations);
    let n = h.len();
    assert_eq!(h[n - 1].x, h[n - 3].x, "period-2 cycle");
    assert_eq!(h[n - 2].x, h[n - 4].x, "period-2 cycle");
    assert_ne!(h[n - 1].x, h[n - 2].x);
    run(
        "HS21",
        &mut Hs21,
        &[-1.0, -1.0],
        &b,
        true,
        -99.96,
        &[2.0, 0.0],
        1e-6,
    );
}

#[test]
fn svanberg_cantilever() {
    // Svanberg 1987 §5.1: f* = 1.340 at x = (6.016, 5.309, 4.494, 3.502, 2.153)
    // from x0 = 5 (SLSQP: 1.3399563606).
    let xs = [6.016_016, 5.309_174, 4.494_330, 3.501_475, 2.152_665];
    for g in [false, true] {
        let mut obj = Cantilever { calls: 0 };
        let st = run(
            "Svanberg cantilever",
            &mut obj,
            &[5.0; 5],
            &Bounds::uniform(5, 1.0, 10.0),
            g,
            1.339_956_360_6,
            &xs,
            1e-4,
        );
        assert_eq!(obj.calls, st.progress.n_evals);
    }
}

#[test]
fn svanberg_two_bar_truss() {
    // Svanberg 1987 §5.2: f* = 1.51 at (1.41, 0.38) from (1.5, 0.5)
    // (SLSQP: 1.5086524175 at (1.4116311, 0.3770724)).
    for g in [false, true] {
        run(
            "Svanberg two-bar truss",
            &mut TwoBarTruss,
            &[1.5, 0.5],
            &Bounds::new(vec![0.2, 0.1], vec![4.0, 1.6]).unwrap(),
            g,
            1.508_652_417_5,
            &[1.411_631_137, 0.377_072_433],
            1e-6,
        );
    }
}

#[test]
fn minimum_compliance_volume_constrained_closed_form() {
    let n = 40;
    let w: Vec<f64> = (0..n)
        .map(|j| 1.0 + (j as f64 * 0.37).sin().powi(2) * 9.0)
        .collect();
    let mut obj = Compliance {
        w,
        volume: 0.5 * n as f64,
    };
    let (xs, fs) = obj.exact();
    for g in [false, true] {
        run(
            "Min-compliance n=40 (closed form)",
            &mut obj,
            &vec![0.5; n],
            &Bounds::uniform(n, 0.01, 2.0),
            g,
            fs,
            &xs,
            1e-6,
        );
    }
}
