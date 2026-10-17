//! A thin adapter from an adjoint VJP to [`geode_optimize::Objective`]
//! (issue #873). It has no physics coupling yet; that is #841 P6b.
//!
//! `geode_core::driven::s_sensitivity::s_matrix_vjp` has this calling
//! convention:
//!
//! ```text
//! s_matrix_vjp::<B, G>(net, omegas, design, opts, objective: G, device)
//!     -> Result<SVjp { objective: f64, grad: Vec<f64>, .. }, SSensitivityError>
//! where G: Fn(usize, &[c64]) -> (f64, Vec<c64>)   // (g_ω(S), ∂g_ω/∂S Wirtinger)
//! ```
//!
//! The parameters are `θ`, and `grad = dg/dθ = 2 Re Σ c_qp ∂S_qp/∂θ`. To
//! optimize with it:
//!
//! 1. Map the optimizer's `x` to the physical `θ`. [`Scaled`] does this
//!    affinely, and chains the gradient exactly.
//! 2. Rebuild the `SDesign` for `θ`, e.g. the per-region ε values or the
//!    amplitudes of the morph's shape columns.
//! 3. Call `s_matrix_vjp` with a band objective closure, and return
//!    `(vjp.objective, vjp.grad)`.
//! 4. Map a solver error to [`EvalError::from_error`]. The optimizer then
//!    backs off from that trial point instead of aborting.
//!
//! The port-mode `vjp` in `analytic::port_mode_sensitivity` plugs in the same
//! way. Its cotangent `cot_beta_sq` comes from `∂g/∂β²` of a `Z₀` / `ε_eff`
//! target, and it returns `dL/dθ`.
//!
//! This example shows the wiring on a stand-in, `mock_s_matrix_vjp`, which
//! has the same calling convention. The stand-in is the 1-port S of a series
//! RLC load on a 1 Ω line, and the example minimizes the band-summed `|S₁₁|²`
//! with L-BFGS-B. The answer is the lowest-Q resonator the bounds allow, with
//! L and C at their bounds and R ≈ 1 Ω. The gradient is FD-checked first, as every `geode
//! optimize` run will do.
//!
//! Run it with `cargo run -p geode-optimize --example s_matrix_vjp_adapter`.

use geode_optimize::{
    Bounds, EvalError, FnObjective, Lbfgsb, LbfgsbOptions, Objective, Optimizer, Scaled,
    StopCriteria, check_gradient,
};
use num_complex::Complex64 as c64;

/// Stand-in for `SSensitivityError`.
#[derive(Debug)]
struct MockError(String);

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MockError {}

/// Stand-in for `SVjp`.
struct MockSVjp {
    objective: f64,
    grad: Vec<f64>,
}

/// Same calling convention as `s_matrix_vjp`: θ = (R, L, C) of a series RLC
/// load, `S₁₁ = (z − 1)/(z + 1)` with `z = R + j(ωL − 1/(ωC))`.
fn mock_s_matrix_vjp<G>(theta: &[f64], omegas: &[f64], objective: G) -> Result<MockSVjp, MockError>
where
    G: Fn(usize, &[c64]) -> (f64, Vec<c64>),
{
    let (r, l, c) = (theta[0], theta[1], theta[2]);
    if !(r > 0.0 && l > 0.0 && c > 0.0) {
        return Err(MockError(format!("non-physical load R={r}, L={l}, C={c}")));
    }
    let j = c64::new(0.0, 1.0);
    let mut total = 0.0;
    let mut grad = vec![0.0; 3];
    for (oi, &w) in omegas.iter().enumerate() {
        let z = r + j * (w * l - 1.0 / (w * c));
        let s = (z - 1.0) / (z + 1.0);
        let ds_dz = 2.0 / ((z + 1.0) * (z + 1.0));
        let dz = [c64::new(1.0, 0.0), j * w, j / (w * c * c)];
        let (g, cot) = objective(oi, &[s]);
        if cot.len() != 1 {
            return Err(MockError("cotangent length".into()));
        }
        total += g;
        for k in 0..3 {
            grad[k] += 2.0 * (cot[0] * ds_dz * dz[k]).re;
        }
    }
    Ok(MockSVjp {
        objective: total,
        grad,
    })
}

fn main() {
    let omegas: Vec<f64> = (0..9).map(|i| 0.8 + 0.05 * f64::from(i)).collect();
    // The band objective g = Σ_ω |S₁₁(ω)|², with Wirtinger cotangent S̄₁₁
    // (the convention of geode-core's `s11_sq_objective`).
    let band = |_: usize, s: &[c64]| (s[0].norm_sqr(), vec![s[0].conj()]);

    // Steps 2–4: the adapter itself.
    let physics = FnObjective::new(3, |theta: &[f64]| {
        mock_s_matrix_vjp(theta, &omegas, band)
            .map(|v| (v.objective, v.grad))
            .map_err(|e| EvalError::from_error(&e))
    });
    // Step 1: optimize in O(1) design units. x ∈ [0, 1]³ maps to
    // R ∈ [0.2, 3], L ∈ [0.2, 5], C ∈ [0.2, 5].
    let lo = [0.2, 0.2, 0.2];
    let hi = [3.0, 5.0, 5.0];
    let span: Vec<f64> = lo.iter().zip(&hi).map(|(a, b)| b - a).collect();
    let mut obj = Scaled::new(physics, lo.to_vec(), span, 1.0);

    let x0 = [0.6, 0.15, 0.7];
    let chk = check_gradient(&mut obj, &x0, 1e-6).expect("FD check");
    println!(
        "adjoint vs central FD at x0: max rel error {:.2e}",
        chk.max_rel_error
    );
    assert!(chk.max_rel_error < 1e-6);

    let opt = Lbfgsb::new(LbfgsbOptions {
        stop: StopCriteria {
            pg_tol: 1e-10,
            ..StopCriteria::default()
        },
        ..LbfgsbOptions::default()
    });
    let st = opt
        .minimize(&mut obj, &x0, &Bounds::uniform(3, 0.0, 1.0))
        .expect("optimize");
    let theta = obj.to_physical(st.x());
    let w0 = 1.0 / (theta[1] * theta[2]).sqrt();
    println!(
        "{:?} after {} iterations / {} solves: R = {:.6}, L = {:.4}, C = {:.4} (resonance ω₀ = {:.4}), Σ|S₁₁|² = {:.3e}",
        st.progress.status,
        st.progress.iter,
        st.progress.n_evals,
        theta[0],
        theta[1],
        theta[2],
        w0,
        st.f()
    );
    // Broadband matching wants the lowest-Q resonator the box allows: L at
    // its lower bound, C at its upper bound (an active-set solution, so the
    // Cauchy-point machinery is exercised), R close to the 1 Ω line, and the
    // resonance in band.
    assert!(st.progress.status.is_converged());
    assert_eq!(st.x()[1], 0.0, "L at its lower bound");
    assert_eq!(st.x()[2], 1.0, "C at its upper bound");
    assert!((theta[0] - 1.0).abs() < 1e-2);
    assert!(w0 > omegas[0] && w0 < omegas[omegas.len() - 1]);
    assert_eq!(obj.dim(), 3);
}

#[test]
fn adapter_example_runs() {
    main();
}
