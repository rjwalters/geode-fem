# Design A adjoint min-max run (issue #1579)

Epic #1252 Phase 4c, child 2 of #1537: the MMA epigraph min-max design run of
the Design A element on the recorded Phase-4 grid, and **the Phase-4
kill-criterion verdict**.

Start point: the hand-designed baseline of #1536
([`../tcda_baseline/`](../tcda_baseline/)). Driver and adjoint: #1578
(`crates/geode-core/tests/common/tcda_design.rs`). Manufacturability rows:
#1535.

## Verdict: **pass**

| quantity | value |
|---|---|
| baseline worst case (#1536) | **-3.50 dB** |
| this run's worst case | **-6.27 dB** |
| improvement | **2.77 dB** |
| kill floor (1 dB better than baseline) | -4.50 dB |
| epic #1252 target | -10.00 dB |
| distance still to go | 3.73 dB |

The kill criterion of #1537 is "improvement over -3.50 dB of less than 1 dB
-> drop the design-result headline". The run improves the worst case by
**2.77 dB**, so the criterion is **not** triggered: the adjoint min-max
method moves this element, and the Phase-4 design-result headline stands.
It does **not** reach the epic's -10 dB, which is a further 3.73 dB away and
is not claimed here.

The worst case moves from 60 deg / H plane / band **top** (baseline) to
60 deg / H plane / band **bottom** (`omega/pi = 0.90`): the optimizer
flattened the band-top edge and the band-bottom edge became binding. Both
E- and H-plane worst cases land within 0.04 dB of each other (-6.31 / -6.27),
i.e. the design is close to equi-ripple across the two planes, which is what
a converged min-max looks like.

## What was run

| | |
|---|---|
| design variables | 52 = `4 * n_edge`, `n_edge = 13` (the 50-200 of #1579) |
| parameterization | **outline only**: spline displacements of the copper outline, millimetres, box `+/- 1 mm` |
| superstrate | **fixed** at the baseline `eps_r = 2.0` (`layers = []`) |
| grid | `ScanGrid::phase4(PI, 0.2, 5)` - 50 points, the recorded #1536 grid |
| feed / reference | the #1536 lumped delta-gap feed, `z_ref = 0.15` |
| footprint | unchanged - same pitch, same stack heights |
| mesh | 10080 tets, 2160 nodes (the #1536 baseline mesh, scaled by `MM_PER_UNIT`) |

Because the superstrate is fixed, `theta0` **is** the recorded baseline: the
run starts at exactly -3.50 dB (the record's `[run].initial_worst_db`, which
reproduced the #1536 number to the printed digit) and every decibel below
that is outline shaping at equal footprint. Tuning `eps_r` or banding the
slab was ruled out of scope by #1579 - see
[`../tcda_baseline/README.md`](../tcda_baseline/README.md) on why the
baseline stack is not a `STANDARD_STACK` laminate.

## Cost, and the budget that was fixed before the run

The example always times one forward pass and one adjoint pass **first** and
prints the projection, so the iteration budget is a stated number rather than
a guess:

| | measured on this host |
|---|---|
| one 50-point forward pass | 227.0 s |
| one adjoint pass (one point) | 5.05 s |
| projected per MMA iteration (50 adjoints) | 479.6 s |
| budget fixed from that | 30 iterations, **5400 s** wall clock |
| actual | **16 iterations, 5639.1 s** (1.57 h) |
| evaluations | 42, of which **25 failed** |
| adjoint passes (`ConstrainedEpigraph::n_gradients()`) | 372 |
| active margin | 0.15 |

`Epigraph::with_active_margin(0.15)` is why 16 iterations cost 372 adjoint
passes rather than 800: only the points within 0.15 (in `|Gamma|^2`) of the
current worst get a gradient. It is a **trade**, not a free saving - a
skipped point is linearized as constant for that iteration - and it is
recorded in `[cost].active_margin` because the result is conditional on it.
The pass-through that exposes it on `ConstrainedEpigraph` is the one driver
change #1579 allowed.

**Stop reason: the wall-clock budget, not convergence.** The optimizer status
was still `Running` at 16 iterations. This is the budget-limited outcome
#1579's AC 2 explicitly accepts, not a failure - but it means -6.27 dB is a
lower bound on what this parameterization can do, not its optimum.

### The recorded design is iteration 13, not iteration 16

MMA is not monotone: it re-expands its asymptotes after a run of small steps.
Iteration 13 reached -6.27 dB, iteration 14 stepped back to -5.30 dB, and the
budget expired at iteration 16 (-5.59 dB). The record therefore keeps the
**best feasible iterate** - lowest true worst case among the iterates whose
#1535 rows are feasible - and names it in `[run].best_iteration`. The full
`t` history is in `[history]` and the true per-iteration worst case is in
`run.log`, so the choice is auditable rather than implicit.

## Acceptance checks at the final design

| check (#1579 AC 2) | result |
|---|---|
| passivity, `\|Gamma\| <= 1 + 1e-6` at all 50 points | **true** |
| no tet inverted (`min_tet_volume_ratio > 0`) | **true**, ratio = 2.8e-5 |
| #1535 manufacturability (`ElementConstraints::is_violated()`) | **false** - feasible, 0.134 mm of margin |

### Caveat: the mesh is valid but one tet is nearly degenerate

`min_tet_volume_ratio = 2.8e-5` is positive, so no element inverted and the
solve is well defined - but the guard's own documentation reads `-> 0+` as
"a tet is nearly degenerate (discretization quality degrading)". One tet of
the morphed mesh has about 1/36000 of its original signed volume.

This does not invalidate the run (the mesh is a valid discretization, every
point is passive, and the pin re-solves the recorded numbers to 0.01 dB), but
**-6.27 dB carries a discretization-quality caveat that this run cannot
settle on its own.** It is exactly the thing #1581 (p=2 and a Palace
cross-check) exists to check, and a future run of this example should
consider a mesh-quality row in the constraint set rather than relying on the
inversion honesty clause alone.

The 25 failed evaluations are the same story seen from the line search: MMA
proposed 42 trial designs and 25 returned an `EvalError` and were rejected.
The driver's realistic failure mode at a trial step is the inverted-morph
honesty clause in `TcdaDesign::morph`, but the run did not log the error
strings, so that attribution is **inferred, not measured**.

## Files

| file | what it is |
|---|---|
| `results.toml` | the machine-written record: design vector, per-point tables, cost, verdict, iteration history |
| `run.log` | the run's own stdout - the cost probe, and the true worst case at every iteration |

`[run].final_theta_mm` is the design vector in the parameterization's own
order (right, left, top, bottom edges, 13 coefficients each), millimetres.
`table_e_plane_db` / `table_h_plane_db` are rows `omega/pi = 0.9 .. 1.1` by
columns `theta = 0,15,30,45,60`, the same layout as
[`../tcda_baseline/results.toml`](../tcda_baseline/results.toml).

### One hand edit, disclosed

`results.toml` is written by the example, with one exception. The example
emitted the `[history]` column that holds `10*log10(t)` under the name
`worst_db`, which is wrong: `t` is the epigraph **bound**, not the achieved
worst case, and the two differ by 2.2 dB at iteration 14 (bound -3.10 dB,
true worst -5.30 dB). The column was renamed to **`t_db`** in the committed
file and in the example, so a regeneration matches. **No value changed** -
every row still satisfies `t_db = 10*log10(t)` from the `t` in that same row,
and the true per-iteration worst case is in `run.log`.

## Reproduce

The regression pin - three of the fifty points, including the recorded worst
one, re-solved against the committed tables within `TOL_DB = 0.05` - runs in
release CI and takes about 20 s:

```sh
cargo test --release -j2 -p geode-core --test tcda_design -- --nocapture \
    the_recorded_design_run_reproduces_its_worst_case
```

The full run is **hours long**. It is an `example`, not a test, precisely so
that no `cargo test` invocation - not even `--include-ignored` - can reach
it:

```sh
# cost probe only (about 4 minutes)
TCDA_DESIGN_TIMING_ONLY=1 cargo run --release -j2 -p geode-core --example tcda_design_run

# the recorded run
TCDA_DESIGN_MAX_ITER=30 TCDA_DESIGN_BUDGET_S=5400 TCDA_DESIGN_ACTIVE_MARGIN=0.15 \
    cargo run --release -j2 -p geode-core --example tcda_design_run
```

Knobs (`TCDA_DESIGN_N_EDGE`, `_MAX_ITER`, `_BUDGET_S`, `_ACTIVE_MARGIN`,
`_BOUND_MM`, `_TIMING_ONLY`, `_DIR`) are documented in the example's module
docs. Timings and the wall-clock stop are host-dependent; the design vector
and the dB tables are not.

## What this does and does not establish

**Does**: on this feed model, this grid and this mesh, the #1578 adjoint plus
MMA improves the Design A worst-case active reflection by 2.77 dB from the
#1536 baseline in 16 budget-limited iterations, at equal footprint, with a
manufacturable outline - so the Phase-4 kill criterion passes.

**Does not**: reach -10 dB (3.73 dB short); converge (the budget stopped it);
establish the result independently of the lumped-feed model the baseline and
this run share (the FEM-vs-oracle mismatch noted in
[`../tcda_baseline/README.md`](../tcda_baseline/README.md) applies to both
ends equally, which is why the comparison is meaningful but the absolute
number is model-bound); or settle the near-degenerate-tet caveat above.
Comparison against CMA-ES is #1580; p=2 and the Palace cross-check are #1581.
