//! Robustness goldens (issue #873):
//! - checkpoint/resume continues **bit-identically** (L-BFGS-B, MMA, GCMMA);
//! - failed evaluations back off rather than abort, and are recorded;
//! - input errors are reported, not panicked on.

mod common;

use common::{Cantilever, Hs71, Rosenbrock, max_abs_diff};
use geode_optimize::{
    Bounds, Control, EvalError, Evaluation, FnObjective, Lbfgsb, LbfgsbOptions, Mma, MmaOptions,
    Objective, OptimizeError, Optimizer, Status, StopCriteria,
};

/// Runs to completion uninterrupted, and again with a pause → JSON →
/// fresh optimizer + state → resume at every iteration `k` in `pauses`.
/// The final checkpoints must be byte-identical (every float is stored
/// bit-exactly, so this is bit-identity of x, f, ∇f, memory and history).
fn assert_bit_identical_resume<O, F>(opt: &O, mut mk: F, x0: &[f64], b: &Bounds, pauses: &[usize])
where
    O: Optimizer,
    F: FnMut() -> Box<dyn Objective>,
{
    let mut obj = mk();
    let full = opt.minimize(obj.as_mut(), x0, b).unwrap();
    let want = opt.checkpoint_json(&full).unwrap();
    let total = O::progress(&full).iter;
    assert!(O::progress(&full).status.is_converged());
    for &k in pauses {
        assert!(k < total, "pause {k} must be before the end ({total})");
        let mut obj = mk();
        let mut st = opt.start(obj.as_mut(), x0, b).unwrap();
        opt.run(obj.as_mut(), &mut st, &mut |s| {
            if O::progress(s).iter >= k {
                Control::Stop
            } else {
                Control::Continue
            }
        })
        .unwrap();
        assert_eq!(
            O::progress(&st).status,
            Status::Running,
            "paused, not finished"
        );
        assert_eq!(O::progress(&st).iter, k);
        let json = opt.checkpoint_json(&st).unwrap();
        drop(st);
        drop(obj);
        // A brand-new process: optimizer and state come only from the JSON.
        let (opt2, mut st2) = O::resume_json(&json).unwrap();
        let mut obj2 = mk();
        opt2.run(obj2.as_mut(), &mut st2, &mut |_| Control::Continue)
            .unwrap();
        let got = opt2.checkpoint_json(&st2).unwrap();
        assert!(
            got == want,
            "resume at iteration {k} diverged from the uninterrupted run"
        );
    }
    eprintln!(
        "| checkpoint/resume {} | {} iterations, paused at {pauses:?} | byte-identical final checkpoint ({} bytes) |",
        O::ALGORITHM,
        total,
        want.len()
    );
}

#[test]
fn lbfgsb_checkpoint_resume_is_bit_identical() {
    let n = 10;
    let x0: Vec<f64> = (0..n)
        .map(|i| if i % 2 == 0 { -1.2 } else { 1.0 })
        .collect();
    let opt = Lbfgsb::new(LbfgsbOptions {
        stop: StopCriteria {
            pg_tol: 1e-9,
            ..StopCriteria::default()
        },
        ..LbfgsbOptions::default()
    });
    // Unbounded and boxed (exercises Cauchy breakpoints and ±inf bounds in JSON).
    assert_bit_identical_resume(
        &opt,
        || Box::new(Rosenbrock::new(n)),
        &x0,
        &Bounds::unbounded(n),
        &[1, 7, 40],
    );
    assert_bit_identical_resume(
        &opt,
        || Box::new(Rosenbrock::new(n)),
        &[2.0; 10],
        &Bounds::uniform(n, 1.2, 3.0),
        &[3, 9],
    );
}

#[test]
fn mma_and_gcmma_checkpoint_resume_is_bit_identical() {
    for globalize in [false, true] {
        let opt = Mma::new(MmaOptions {
            globalize,
            stop: StopCriteria {
                pg_tol: 1e-7,
                feas_tol: 1e-9,
                ..StopCriteria::default()
            },
            ..MmaOptions::default()
        });
        assert_bit_identical_resume(
            &opt,
            || Box::new(Hs71),
            &[1.0, 5.0, 5.0, 1.0],
            &Bounds::uniform(4, 1.0, 5.0),
            &[1, 2, 5],
        );
        assert_bit_identical_resume(
            &opt,
            || Box::new(Cantilever { calls: 0 }),
            &[5.0; 5],
            &Bounds::uniform(5, 1.0, 10.0),
            &[2, 6],
        );
    }
}

/// Negative control for the bit-identity test: a 1e-12 relative change to a single
/// piece of state (L-BFGS `θ`, an MMA gradient entry) in the JSON
/// must change the continued run. This shows the comparison is sensitive
/// to the state it claims to round-trip.
#[test]
fn tiny_state_perturbation_is_detected_by_the_resume_comparison() {
    use geode_optimize::exact::{format_f64, parse_f64};
    fn bump_first(json: &str, key: &str) -> String {
        let at = json.find(&format!("\"{key}\": ")).expect("key present");
        let start = at + key.len() + 4;
        let open = json[start..].find('"').unwrap() + start + 1;
        let close = json[open..].find('"').unwrap() + open;
        let v = parse_f64(&json[open..close]).unwrap();
        let bumped = v * (1.0 + 1e-12);
        format!("{}{}{}", &json[..open], format_f64(bumped), &json[close..])
    }
    // L-BFGS-B: θ after 5 iterations.
    let opt = Lbfgsb::default();
    let x0 = [-1.2, 1.0, -1.2, 1.0];
    let full = opt
        .minimize(&mut Rosenbrock::new(4), &x0, &Bounds::unbounded(4))
        .unwrap();
    let mut st = opt
        .start(&mut Rosenbrock::new(4), &x0, &Bounds::unbounded(4))
        .unwrap();
    opt.run(&mut Rosenbrock::new(4), &mut st, &mut |s| {
        if s.progress.iter >= 5 {
            Control::Stop
        } else {
            Control::Continue
        }
    })
    .unwrap();
    let json = bump_first(&opt.checkpoint_json(&st).unwrap(), "theta");
    let (o2, mut s2) = <Lbfgsb as Optimizer>::resume_json(&json).unwrap();
    o2.run(&mut Rosenbrock::new(4), &mut s2, &mut |_| Control::Continue)
        .unwrap();
    assert_ne!(
        opt.checkpoint_json(&full).unwrap(),
        o2.checkpoint_json(&s2).unwrap()
    );
    // MMA: the first objective-gradient entry after 6 iterations (it sets the
    // subproblem's p₀ⱼ, q₀ⱼ, so the next iterate). A one-ulp change is too
    // small a probe here: it is absorbed by rounding (an asymptote is also
    // re-clamped), so the probe is 1e-12 relative.
    let opt = Mma::default();
    let b = Bounds::uniform(5, 1.0, 10.0);
    let full = opt
        .minimize(&mut Cantilever { calls: 0 }, &[5.0; 5], &b)
        .unwrap();
    let mut st = opt
        .start(&mut Cantilever { calls: 0 }, &[5.0; 5], &b)
        .unwrap();
    opt.run(&mut Cantilever { calls: 0 }, &mut st, &mut |s| {
        if s.progress.iter >= 6 {
            Control::Stop
        } else {
            Control::Continue
        }
    })
    .unwrap();
    let cp = opt.checkpoint_json(&st).unwrap();
    let json = bump_first(&cp, "grad");
    let (o2, mut s2) = <Mma as Optimizer>::resume_json(&json).unwrap();
    o2.run(&mut Cantilever { calls: 0 }, &mut s2, &mut |_| {
        Control::Continue
    })
    .unwrap();
    assert_ne!(
        opt.checkpoint_json(&full).unwrap(),
        o2.checkpoint_json(&s2).unwrap()
    );
}

#[test]
fn checkpoint_rejects_the_wrong_algorithm_and_garbage() {
    let opt = Lbfgsb::default();
    let mut obj = Rosenbrock::new(2);
    let st = opt
        .start(&mut obj, &[0.0, 0.0], &Bounds::unbounded(2))
        .unwrap();
    let json = opt.checkpoint_json(&st).unwrap();
    assert!(
        json.contains("\"-inf\"") && json.contains("\"inf\""),
        "±inf bounds survive"
    );
    assert!(matches!(
        <Mma as Optimizer>::resume_json(&json),
        Err(OptimizeError::Checkpoint(_))
    ));
    assert!(matches!(
        <Lbfgsb as Optimizer>::resume_json("{not json"),
        Err(OptimizeError::Checkpoint(_))
    ));
}

/// Rosenbrock that fails (like a singular forward solve) inside a disc that
/// the very first trial step lands in, from (−1.2, 1).
struct HoleyRosenbrock {
    calls: usize,
}

impl Objective for HoleyRosenbrock {
    fn dim(&self) -> usize {
        2
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        self.calls += 1;
        let r = ((x[0] + 0.3).powi(2) + (x[1] - 1.4).powi(2)).sqrt();
        if r < 0.3 {
            return Err(EvalError::new(format!(
                "singular operator at x = ({:.3}, {:.3})",
                x[0], x[1]
            )));
        }
        let (f, g) = common::rosenbrock(x);
        Ok(Evaluation::unconstrained(f, g))
    }
}

#[test]
fn lbfgsb_backs_off_from_failed_evaluations_and_converges() {
    let mut obj = HoleyRosenbrock { calls: 0 };
    let st = Lbfgsb::new(LbfgsbOptions {
        stop: StopCriteria {
            pg_tol: 1e-9,
            ..StopCriteria::default()
        },
        ..LbfgsbOptions::default()
    })
    .minimize(&mut obj, &[-1.2, 1.0], &Bounds::unbounded(2))
    .unwrap();
    let p = &st.progress;
    assert!(p.status.is_converged(), "{:?}", p.status);
    assert!(max_abs_diff(&p.x, &[1.0, 1.0]) < 1e-7);
    assert!(
        p.n_failed_evals >= 1,
        "the first trial step must have failed"
    );
    assert_eq!(p.n_evals, obj.calls);
    let first = &p.history[1];
    assert!(first.failed_this_iter >= 1);
    assert!(
        first.notes.iter().any(|n| n.contains("singular operator")),
        "{:?}",
        first.notes
    );
    let recorded: usize = p.history.iter().map(|h| h.failed_this_iter).sum();
    assert_eq!(recorded, p.n_failed_evals);
    eprintln!(
        "| L-BFGS-B failed-eval backoff (Rosenbrock with a failing disc) | {} iterations, {} evals, {} failed | converged to (1, 1) |",
        p.iter, p.n_evals, p.n_failed_evals
    );
}

/// Wraps an objective and fails on chosen call numbers (a flaky solver).
struct Flaky<O> {
    inner: O,
    calls: usize,
    fail_on: Vec<usize>,
}

impl<O: Objective> Objective for Flaky<O> {
    fn dim(&self) -> usize {
        self.inner.dim()
    }
    fn n_constraints(&self) -> usize {
        self.inner.n_constraints()
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        self.calls += 1;
        if self.fail_on.contains(&self.calls) {
            return Err(EvalError::new(format!(
                "solver flake on call {}",
                self.calls
            )));
        }
        self.inner.evaluate(x)
    }
}

#[test]
fn mma_and_gcmma_back_off_from_failed_evaluations() {
    for globalize in [false, true] {
        let mut obj = Flaky {
            inner: Cantilever { calls: 0 },
            calls: 0,
            fail_on: vec![2, 3, 7],
        };
        let st = Mma::new(MmaOptions {
            globalize,
            stop: StopCriteria {
                pg_tol: 1e-7,
                feas_tol: 1e-9,
                ..StopCriteria::default()
            },
            ..MmaOptions::default()
        })
        .minimize(&mut obj, &[5.0; 5], &Bounds::uniform(5, 1.0, 10.0))
        .unwrap();
        let p = &st.progress;
        assert!(p.status.is_converged(), "{:?}", p.status);
        assert!((p.f - 1.339_956_360_6).abs() < 1e-8);
        assert_eq!(p.n_failed_evals, 3);
        assert_eq!(p.n_evals, obj.calls);
        assert!(p.history[1].failed_this_iter >= 2);
        assert!(
            p.history[1]
                .notes
                .iter()
                .any(|n| n.contains("solver flake on call 2"))
        );
        if !globalize {
            assert_eq!(
                p.history[1].step_length, 0.25,
                "two halvings of the MMA step"
            );
        }
        eprintln!(
            "| {} failed-eval backoff (cantilever, calls 2, 3, 7 fail) | {} iterations, {} evals, 3 failed | f = {:.10} |",
            if globalize { "GCMMA" } else { "MMA" },
            p.iter,
            p.n_evals,
            p.f
        );
    }
}

#[test]
fn persistent_failure_stalls_with_a_message_instead_of_aborting() {
    // Every evaluation after the first fails.
    let mut calls = 0;
    let mut obj = FnObjective::new(2, move |x: &[f64]| {
        calls += 1;
        if calls > 1 {
            Err(EvalError::new("mesh inverted"))
        } else {
            let (f, g) = common::rosenbrock(x);
            Ok((f, g))
        }
    });
    let st = Lbfgsb::default()
        .minimize(&mut obj, &[-1.2, 1.0], &Bounds::unbounded(2))
        .unwrap();
    match &st.progress.status {
        Status::Stalled { message } => assert!(message.contains("line search failed")),
        s => panic!("expected Stalled, got {s:?}"),
    }
    assert_eq!(st.progress.x, vec![-1.2, 1.0], "the iterate is unchanged");
    assert!(st.progress.n_failed_evals > 0);

    let mut obj = Flaky {
        inner: Cantilever { calls: 0 },
        calls: 0,
        fail_on: (2..100).collect(),
    };
    let st = Mma::default()
        .minimize(&mut obj, &[5.0; 5], &Bounds::uniform(5, 1.0, 10.0))
        .unwrap();
    assert!(matches!(st.progress.status, Status::Stalled { .. }));
    assert_eq!(
        st.progress.n_failed_evals,
        MmaOptions::default().max_backoff + 1
    );
}

#[test]
fn input_errors_are_reported() {
    // Failure at the starting point: nothing to back off to.
    let mut bad = FnObjective::new(1, |_: &[f64]| Err(EvalError::new("no mesh")));
    assert!(matches!(
        Lbfgsb::default().minimize(&mut bad, &[0.0], &Bounds::unbounded(1)),
        Err(OptimizeError::InitialEvaluation(_))
    ));
    // L-BFGS-B refuses general constraints; MMA refuses infinite bounds.
    assert!(matches!(
        Lbfgsb::default().minimize(&mut Hs71, &[1.0; 4], &Bounds::uniform(4, 1.0, 5.0)),
        Err(OptimizeError::InvalidInput(_))
    ));
    assert!(matches!(
        Mma::default().minimize(&mut Hs71, &[1.0; 4], &Bounds::unbounded(4)),
        Err(OptimizeError::InvalidInput(_))
    ));
    // Dimension mismatches and inverted bounds.
    assert!(matches!(
        Lbfgsb::default().minimize(&mut Rosenbrock::new(3), &[0.0; 2], &Bounds::unbounded(3)),
        Err(OptimizeError::InvalidInput(_))
    ));
    assert!(Bounds::new(vec![1.0], vec![0.0]).is_err());
    // A wrong-length gradient is a failed evaluation (here at the start).
    let mut short = FnObjective::new(2, |_: &[f64]| Ok((0.0, vec![0.0])));
    assert!(matches!(
        Lbfgsb::default().minimize(&mut short, &[0.0, 0.0], &Bounds::unbounded(2)),
        Err(OptimizeError::InitialEvaluation(_))
    ));
    // x0 outside the box is projected in.
    let st = Lbfgsb::default()
        .start(
            &mut Rosenbrock::new(2),
            &[9.0, -9.0],
            &Bounds::uniform(2, -2.0, 2.0),
        )
        .unwrap();
    assert_eq!(st.progress.x, vec![2.0, -2.0]);
}
