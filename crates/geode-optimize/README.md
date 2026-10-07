# geode-optimize

The gradient-based optimizer core of the GEODE-FEM design loop (Epic #841,
Phase 6a, issue #873). It is pure Rust and physics-independent. It has no
dependency on `burn`, `faer` or `geode-core`, only `serde`, `serde_json` and
`thiserror`. A caller couples it to an adjoint solve by implementing one trait,
`Objective`, which returns the figure of merit, its gradient and, for MMA, the
constraint values and Jacobian rows from a single evaluation.

| Item | Source |
|---|---|
| `Lbfgsb`: L-BFGS-B for box-bounded variables, the default optimizer | [`src/lbfgsb.rs`](src/lbfgsb.rs), [`src/line_search.rs`](src/line_search.rs) |
| `Mma`: Method of Moving Asymptotes for inequality constraints, with an optional GCMMA inner loop | [`src/mma.rs`](src/mma.rs) |
| `Objective`, `FnObjective`, `Scaled`, `EvalError` | [`src/objective.rs`](src/objective.rs) |
| `Optimizer::step` / `run`, `StopCriteria`, iteration history | [`src/driver.rs`](src/driver.rs) |
| Bit-exact JSON checkpoint and resume | [`src/exact.rs`](src/exact.rs) |

A failed evaluation (for example a forward solve that errors at a trial
geometry) makes both optimizers back off instead of aborting, and the failure
is recorded in the history. A resumed run reproduces the uninterrupted run bit
for bit.

```rust
use geode_optimize::{Bounds, FnObjective, Lbfgsb, Optimizer};

// Minimize (x − 3)² + (y + 1)² on the box [0, 2] × [−5, 5].
let mut obj = FnObjective::new(2, |x: &[f64]| {
    let f = (x[0] - 3.0).powi(2) + (x[1] + 1.0).powi(2);
    Ok((f, vec![2.0 * (x[0] - 3.0), 2.0 * (x[1] + 1.0)]))
});
let bounds = Bounds::new(vec![0.0, -5.0], vec![2.0, 5.0]).unwrap();
let st = Lbfgsb::default().minimize(&mut obj, &[0.5, 0.5], &bounds).unwrap();
assert!(st.progress.status.is_converged());
```

[`examples/s_matrix_vjp_adapter.rs`](examples/s_matrix_vjp_adapter.rs) shows how
to wire an adjoint VJP with the calling convention of
`geode_core::driven::s_sensitivity::s_matrix_vjp` into an `Objective`, using a
mock. The goldens and robustness tests are in [`tests/`](tests/):

```sh
cargo test -p geode-optimize
```
