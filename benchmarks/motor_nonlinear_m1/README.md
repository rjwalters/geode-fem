# Motor paper M1: nonlinear B-H scalar magnetostatics (issue #1157)

This directory holds the validation record for
`crates/geode-core/src/assembly/magnetostatic_nonlinear.rs`. That module
solves P1 nonlinear 2-D scalar magnetostatics in SI units with a
consistent-tangent damped Newton method. It provides a Brauer law and a
monotone C¹ tabulated B-H law, and it exposes the converged tangent for the
M3 adjoint.

- `m1_tables.log` is the full `--nocapture` output of the acceptance tests.
  Regenerate it with:

  ```text
  cargo test -p geode-core --release --test magnetostatic_nonlinear -- --nocapture --test-threads=1
  ```

  The tables below are copied from that log. They are deterministic: there
  is a fixed-seed random direction and no timing data. The test file is
  `crates/geode-core/tests/magnetostatic_nonlinear.rs`, and CI runs it in
  `integration-tests.yml` (core-tests-a).
- The B-H table in the tests is **synthetic**. It is a realistic
  non-oriented electrical-steel shape (initial μ_r ≈ 4800, knee at
  1.3–1.5 T, 2.28 T at 200 kA/m), not measured data and not tied to any
  grade. The Brauer coefficients are `ν = 3.8·exp(2.17 b²) + 396.2` m/H.

## Units

The module is SI throughout: ν in m/H, J in A/m², A_z in Wb/m, B in T.
The linear module `assembly::magnetostatic` solves
`−∇·(ν_r ∇A) = μ₀ J` with `ν_r = 1/μ_r` and a source of
`μ₀ ∫ M·(∇φ)^⊥`. Dividing by μ₀ gives `ν_SI = ν_r/μ₀`, `J_SI = J`, and a
source of `∫ M·(∇φ)^⊥`, with the same A_z. Test 1 confirms this.

## 1. Linear limit (constant laws against `assemble_magnetostatic` + `solve`)

The setup is a five-band machine cross-section: an iron bore with μ_r 1000,
a magnet band with μ_rec 1.05, then air gap, winding and outer air.

| case | max\|ΔA\|/max\|A\| | Newton iterations | ‖R₁‖/‖R₀‖ |
|---|---|---|---|
| current only | 1.3e-14 | 1 | 5.4e-14 |
| current + PM magnetization | 1.7e-14 | 1 | 5.8e-14 |

## 2. Tangent check (central FD of R against K·v, random v)

The state is a 16×16 all-iron square, 5 cm on a side, with
`A = s·L·e^{y/L} sin(2x/L+0.5)`. The step h is the relative perturbation of
the field: the direction is scaled so its largest element |∇v| equals the
state's largest |B|. The table law is C¹, so H'' jumps at its knots. An
element whose |B| crosses a knot inside the FD stencil has an O(h) local
truncation error. The "knot-free" column drops the rows touched by elements
that cross a knot at h = 1e-2. It uses the same row set at every h.

| h | Brauer (\|B\| 0.41–1.91 T) | table, all rows (\|B\| 0.52–2.38 T) | table, knot-crossing elements | table, knot-free rows |
|---|---|---|---|---|
| 1e-2 | 1.007e-3 | 1.054e-3 | 49 | 3.688e-4 |
| 1e-3 | 1.007e-5 | 5.820e-6 | 3 | 3.688e-6 |
| 1e-4 | 1.007e-7 | 6.530e-8 | 0 | 3.688e-8 |
| 1e-5 | 1.059e-9 | **6.552e-10** | 0 | 3.975e-10 |
| 1e-6 | **8.258e-10** | 1.322e-9 | 0 | 7.628e-10 |
| 1e-7 | 7.079e-9 | 8.088e-9 | 0 | 1.272e-8 |
| 1e-8 | 1.077e-7 | 9.042e-8 | 0 | 9.737e-8 |

The best error is 8.3e-10 for Brauer and 6.6e-10 for the table, against a
1e-7 target. The observed order between consecutive steps from 1e-2 to 1e-5
is 2.00, 2.00 and 1.98 for Brauer. For the table it is 2.26, 1.95 and 2.00
on all rows, and 2.00, 2.00 and 1.97 on the knot-free rows.

Other checks on the laws:
- `dν/d(b²)` against an FD of ν, and the energy density against
  `∫ν/2`, agree to ≤2.1e-10 for both laws over 0–2.8 T.
- The table interpolates every point, and `b_of_h` inverts it to 1e-12.
- The 1/μ₀ tail joins with a continuous slope.
- The reduced tangent is symmetric to 1.5e-17 relative and SPD
  (λ_min > 0) at saturated states for both laws.

## 3. Newton convergence against the frozen-ν fixed point

The case is the coax of section 5 at n_ang = 64 (3329 nodes), in deep
saturation, solved from zero with the default configuration (energy line
search, rel_tol 1e-8).

| k | Newton, table: ‖R_k‖/‖R₀‖ | α_k | Newton, Brauer: ‖R_k‖/‖R₀‖ | α_k |
|---|---|---|---|---|
| 1 | 8.77e1 | 0.0156 | 9.86e-1 | 0.0156 |
| 2 | 8.64e1 | 0.0156 | 1.62e2 | 0.0312 |
| 3 | 8.37e1 | 0.0312 | 1.52e2 | 0.0625 |
| 4 | 7.85e1 | 0.0625 | 1.34e2 | 0.125 |
| 5 | 6.88e1 | 0.125 | 1.05e2 | 0.25 |
| 6 | 9.17e0 | 1 | 8.42e1 | 1 |
| 7 | 2.05e0 | 1 | 3.15e1 | 1 |
| 8 | 2.60e-1 | 1 | 1.17e1 | 1 |
| 9 | 8.11e-3 | 1 | 4.10e0 | 1 |
| 10 | 1.48e-5 | 1 | 1.27e0 | 1 |
| 11 | 8.45e-11 (floor) | 1 | 2.94e-1 | 1 |
| 12 | | | 2.70e-2 | 1 |
| 13 | | | 2.42e-4 | 1 |
| 14 | | | 1.94e-8 | 1 |
| 15 | | | 7.15e-11 (floor) | 1 |

- **Brauer (C^∞):** over the terminal transitions, C_k = r_k/r_{k−1}²
  stays at 0.312–0.332, and the consecutive orders are 1.97 and 2.00. That
  is clean quadratic convergence.
- **Table (C¹):** over the terminal transitions, C_k is 0.062–0.226, the
  consecutive orders are 1.68 and 1.82, and the fitted order is 1.77. The
  convergence is quadratic in the pre-asymptotic regime: the orders are
  still rising when the residual reaches the round-off floor. The test
  asserts more than three decades of drop, C_k bounded within a factor of
  10, and a fitted order above 1.7.
- **Frozen ν (Picard) on the table case** needs 138 iterations, with a
  steady ratio of about 0.86 (0.8663 geometric mean over the last 40) and a
  fitted order of 1.000. That is linear convergence.

The round-off floor is about 1e-10 relative. It comes from cancellation in
∇A over small elements when A has a large offset. The default rel_tol is
therefore 1e-8.

The converged tangent drives the solution sensitivity:
`dA/dI = K⁻¹F/I`, computed with `solve_tangent_transpose`, matches a
central FD of two converged solves (dI/I = 1e-3) to 8.0e-7.

## 4. Manufactured solution

The case is Brauer ν on [0,1]² with `A* = 0.35 e^y sin(2x+0.5)` and
non-homogeneous Dirichlet data from A*. The load is
`f = −∇·(ν∇A*)`, integrated with the degree-4 rule. The solve starts from
the ν(0) linear start: with a zero interior and non-zero boundary values,
the exponential law overflows at the boundary layer.

| n | h | \|A−A*\|_H¹ | rate | ‖A−A*‖_L² | rate | Newton iterations | ν range (m/H) |
|---|---|---|---|---|---|---|---|
| 8 | 1.25e-1 | 9.182e-2 | | 1.855e-3 | | 5 | 401–1363 |
| 16 | 6.25e-2 | 4.574e-2 | 1.01 | 4.553e-4 | 2.03 | 8 | 401–1990 |
| 32 | 3.13e-2 | 2.283e-2 | 1.00 | 1.134e-4 | 2.01 | 11 | 401–2417 |
| 64 | 1.56e-2 | 1.141e-2 | 1.00 | 2.832e-5 | 2.00 | 13 | 401–2662 |
| 128 | 7.81e-3 | 5.703e-3 | 1.00 | 7.079e-6 | 2.00 | 15 | 401–2793 |

The least-squares fitted rates are 1.002 for the H¹ seminorm and 2.007 for
L². The test asserts H¹ in [0.95, 1.1] and L² in [1.9, 2.2].

## 5. Saturated-iron Ampère oracle

The geometry is a conductor with r < 4 mm carrying I = 2π·5 mm·40 kA/m =
1256.6 A, an air gap, an iron annulus from 5 to 250 mm (synthetic table),
and air out to a Dirichlet ring at 300 mm. The mesh is log-graded, with
Δr/r ≈ 2π/n_ang in the iron. The oracle is
`|B|(r) = B_of_H(I/(2πr))`, compared element by element at the centroid
radius. The current density is normalized by the meshed conductor area, so
I_enc is exact. "Mid-annulus" means `|ln(r/√(r_i r_o))| < 0.1`, that is
r ≈ 35 mm with |B| ≈ 1.63 T.

| n_ang | nodes | Newton iterations (energy / residual LS) | max rel err | RMS rel err | mid-annulus mean rel err | \|B\| in iron (T) |
|---|---|---|---|---|---|---|
| 32 | 897 | 11 / 16 | 3.19e-3 | 2.57e-3 | 2.38e-3 | 1.428–1.922 |
| 64 | 3329 | 11 / 16 | 1.58e-3 | 1.29e-3 | 1.21e-3 | 1.424–1.928 |
| 128 | 13441 | 11 / 17 | 7.86e-4 | 6.45e-4 | 6.03e-4 | 1.422–1.930 |
| 256 | 52993 | 12 / 19 | 3.95e-4 | 3.24e-4 | **3.03e-4** | 1.421–1.931 |

The error converges at O(h): it halves with each doubling of n_ang. The
inner iron sits at 1.92–1.93 T (deep saturation) and the outer at 1.42 T
(the knee). The finest mid-annulus error is 0.03%, against a 1% target.

The tripwire uses linear iron at the table's initial μ_r. It misses the
mid-annulus oracle by 1951%, against 0.24% for the nonlinear solve.

## 6. Law validation

Each of these tables is rejected with `InvalidLaw` and a specific message:
- mismatched lengths;
- a single point;
- not starting at the origin;
- a repeated B;
- non-monotone H;
- a NaN;
- a table that stops short of saturation, so that a C¹ join to the 1/μ₀
  tail cannot be monotone.

Invalid Brauer coefficients and ConstantLaw values are rejected too. An
unmapped region tag is `MissingMaterial`, and an iteration cap reached is
`NotConverged` with the residual history attached. No silent last iterate
is returned.
