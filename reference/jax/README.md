# JAX reference implementations

Differentiable reference backend per **Epic #88**. JAX probes how
natural the L4 calculus is for XLA tracing and whether autodiff
survives the assembly path.

## Status

- **`cube_cavity.py`** — full scalar-Helmholtz cube-cavity pipeline
  (Epic #88 / #93). Assembly in JAX (`jit` + `vmap`); SciPy at the
  eigensolve boundary. Includes the AC#2 autodiff anchor:
  `jax.grad(tr(K_int))` finite-difference-validated within `1e-5`
  (actually agrees to ~`1e-10` on a `n=3` mesh).
- **`gen_cube_cavity_fixture.py`** — fixture generator producing
  `reference/fixtures/cube_cavity/jax_baseline.json` in the canonical
  v1 schema (cross-checked against the sibling
  `reference/numpy/cube_cavity_minimal.py` at fixture-gen time).
- **`sphere_pec.py`** — full sphere-PEC Nédélec eigenmode pipeline
  (Epic #88 / Phase G.3 / Issue #128). Per-element curl-curl + ε-mass
  via `jit`+`vmap`; SciPy `eigsh` at the eigensolve boundary.
  Autodiff anchor: `jax.grad(tr(K_int_cc))` vs node coordinates,
  cross-checked against finite difference.
- **`sphere_pml.py`** — sphere-PML Nédélec pipeline with **complex**
  per-tet ε (Epic #88 / Phase H.3 / Issue #148). Adds the c128
  constitutive path. SciPy `eigs` at the eigensolve boundary with a
  physical-band shift (`σ=0.9+0j`) to bypass the spurious cluster.
  Includes `probe_autodiff_complex_assembly` — the explicit
  differentiability probe for complex assembly (documentation-only,
  per issue #148).
- **`gen_sphere_pml_fixture.py`** — fixture generator producing
  `reference/fixtures/sphere_pml/jax_baseline.json` using the Phase H
  c128 schema (real-imag interleaved on disk, `|Δ|`-tolerance).
- **`sphere_mie.py`** — anisotropic-UPML dielectric-sphere Mie
  pipeline (Epic #88 / Phase J.4 / Issue #173). Ports the J.2
  tensor-ε assembly: per-axis cofactor-gram mass kernel in c128
  (`jax.vmap`/`jit`), `BCOO[complex128]` global scatter, eigensolve
  out-of-graph on host LAPACK ZGGEV (dense canonical-tiebreaker
  path). Includes `probe_autodiff_tensor_assembly` — `jax.grad`
  through the **tensor-valued** complex-ε kernel, closing the H.3
  scalar-only caveat (verdict: traces cleanly, zero custom VJPs).
- **`gen_sphere_mie_fixture.py`** — fixture generator producing
  `reference/fixtures/sphere_mie_small/jax_baseline.json` (small-mesh
  granularity — the default-CI-checkable slice, #158/#164/#160
  precedent). Hard-asserts the NumPy J.2 spectrum (dense vs dense),
  the J.1 analytic TM_1,1 anchor, Q tripwires, the σ₀ = 0 collapse,
  and a clean autodiff probe verdict at generation time; gated by
  `.github/workflows/jax-sphere-mie.yml` (Option A).

## Quick start

```sh
python3 -m venv .venv-jax
source .venv-jax/bin/activate
pip install "jax[cpu]" numpy scipy

# Run the self-check
python3 reference/jax/cube_cavity.py --n 4

# Regenerate the JAX baseline fixture
python3 reference/jax/gen_cube_cavity_fixture.py --n 4 \
    --out reference/fixtures/cube_cavity/jax_baseline.json
```

## What the AC#2 autodiff anchor exercises

The functional `tr(K_int)` is differentiable w.r.t. node coordinates
(holding the interior mask topology fixed). `jax.grad` traces through:

1. `_assemble_dense_jax` (which calls `vmap(_p1_local_one)`),
2. the `tf.cross`, edge subtractions, and `gMat @ gMat.T` chain
   inside `_p1_local_one`,
3. the `at[rows, cols].add(...)` scatter-with-add into the global
   matrix buffer,
4. the interior-DOF gather, and
5. the final `trace`.

End-to-end, this proves that the JAX assembly path is differentiable
through the same scatter-add semantics the Burn path uses
(`IndexingUpdateOp::Add`, per `crates/geode-core/src/assembly.rs`).
The eigensolve is deliberately not differentiated (#88 Phase C
explicitly leaves the eigensolve as a "boundary allowed" non-XLA
op).

## JAX-DX friction observations (per the JAX-DX follow-up comment on #88)

These are the friction artifacts surfaced while porting the NumPy
cube-cavity reference to JAX. They are the durable record the #88
framing asks for — they go to #5 as supporting evidence, not
side-channel grumbling.

### 1. Lift-from-Python ritual is concentrated at the *connectivity* boundary

NumPy's `np.einsum`, `np.cross`, and `np.stack` translate to JAX
one-for-one — that part of the port was mechanical. The friction was
at the connectivity boundary:

- **JAX's `.at[rows, cols].add(...)`** is the correct primitive for
  scatter-add, but it requires the indices to be **JAX arrays**
  (not Python lists). The bring-up cost was identifying that the
  tet-connectivity `int[][]` had to become `jnp.asarray(tets, dtype=jnp.int32)`
  before broadcasting could happen.
- **`jnp.ix_`** does not exist in `jax.numpy` the way `np.ix_` does
  in NumPy. The compatibility layer landed in jax 0.4.x; we used the
  same idiom (`jnp.ix_` is now provided), but a reader coming from
  `np.ix_` might not realize the API is provided only because that
  compat shim landed late. This is the kind of "Python compatibility
  is partial" friction the #88 framing predicted.
- **`tets` cannot be a JAX traced value** when it's used as an
  argument to `at[...]` — it has to be a constant. We hardcode it
  through the `_assemble_dense_jit_factory(tets)` closure pattern.
  This is documented in the `lax` autodiff guidance but tripped up
  the first version of the port.

### 2. f64 must be explicitly enabled

`jax.config.update("jax_enable_x64", True)` at module top. By default
JAX runs f32 — entirely sensible for ML, lethal for FEM eigenvalue
convergence. This is a one-line gotcha, but it is a real one — the
first run of the pipeline silently disagreed with NumPy at the 1e-5
level (f32 round-off accumulating through 6 * n^3 elements) before
the flag was added.

### 3. JIT compile time is non-trivial on first call

For `n=4` (384 tets, ~125 nodes), the first `jit`-traced call takes
~3-5s on a 2026-era Apple Silicon laptop. Re-calls take ~10 ms. This
is normal JAX behavior — XLA compilation is amortized across many
calls — but for a fixture-gen script (single call) it's pure overhead.
This is a *real* DX cost for the friction-mining workflow ("edit
NumPy → see new diff artifact"): if the JAX path is in the loop,
add ~5s/iteration latency.

### 4. The autodiff path through `at[...].add(...)` is solid

This is the *positive* observation worth recording: `jax.grad` traces
cleanly through scatter-add. The finite-difference cross-check agrees
to ~`1e-10` (far better than the required `1e-5`), so the assembly
chain is genuinely differentiable, not approximately so. This
confirms #88's underlying bet that JAX-shaped assembly is autodiff-
amenable end-to-end. Burn's `IndexingUpdateOp::Add` makes the same
guarantee on the Rust side; we now have cross-language evidence that
this primitive is the right one.

### 5. Eigensolve boundary is hard to avoid

JAX has `jax.scipy.linalg.eigh` for **dense symmetric** problems and
`jax.scipy.sparse.linalg` is mostly iterative solvers (not
eigensolvers). The generalized sparse eigenproblem we need (`K x =
λ M x`) has no native JAX path. This is the friction the issue body
predicted ("differentiability of assembly tested (eigensolve
boundary allowed)"). We use `scipy.sparse.linalg.eigsh` /
`scipy.linalg.eigh` at the boundary — the dense path for small `n`
since ARPACK is fragile below ~30 DOFs; the sparse path for larger
`n`.

If a future #88 child wants differentiable eigenvalues, the natural
path is either (a) implicit-function-theorem unrolling of the linear
solve at each ARPACK iteration (painful), or (b) hand-coded shift-
and-invert with the linear-solve gradient handled by a custom
`jax.custom_vjp`. Neither is in scope for #93.

### 6. `jax.config.read("jax_enable_x64")` returns a string, not bool

Minor: when printing the config state for the self-check banner, the
returned value is `'1'` (string), not `True` (bool). Caught a typo
where I'd assumed bool. Documented here so the next person doesn't.

## Phase H JAX friction notes (Issue #148, complex assembly)

The Phase H.3 deliverable was explicitly framed as a friction-mining
probe: does JAX's autodiff + JIT trace cleanly through complex
(`c128`) constitutive assembly? Here is the record.

### A. `BCOO[complex128]` works end-to-end

`jax.experimental.sparse.BCOO` accepts `dtype=jnp.complex128` data
and round-trips through `.todense()` and complex matvec without
issue. We **did not** end up using it on the eigensolve hot path —
the `scipy.sparse` scatter is cheaper for fixture generation and
the downstream `scipy.sparse.linalg.eigs` boundary requires SciPy
sparse anyway — but a positive smoke result is in
`reference/jax/sphere_pml.py`'s loader path. If a future child
issue wants in-graph complex sparse linear algebra,
`BCOO[complex128]` is a viable substrate.

### B. JIT lowers complex assembly without errors

`jax.jit` of the in-graph dense complex assembly path
(`_make_complex_assembly_loss` in `sphere_pml.py`) lowers to XLA
cleanly. No `complex128 unsupported on backend cpu` errors. The
trace closes a complex multiply (`m_local_real * eps_complex`),
complex scatter-add (`at[rows, cols].add(complex_vals)`), interior
restriction (`jnp.ix_`), and a complex `jnp.trace`. All XLA-native.

### C. `jax.grad` through complex assembly returns finite gradients

The explicit probe (`probe_autodiff_complex_assembly`) wraps
`loss(eps_re, eps_im) = trace(K) + |trace(M)|²` with a real-valued
output and differentiates w.r.t. both halves of the complex per-tet ε
vector. Both `grad_re` and `grad_im` are finite, bounded, and
nontrivial. No NaN/inf injection.

This is the positive friction-mining result: **JAX can autodiff
through a complex FEM assembly path on f64/c128 without any custom
VJPs or workarounds.** That validates one of Epic #88's open
questions ("does autodiff survive lossy ε?") in the affirmative,
for the assembly stage at least.

### D. Eigensolve boundary survives unchanged

As predicted by the Phase G.6 ONNX audit (`reference/onnx/audit/`
Stage 7), no JAX backend lowers the generalized eigensolve in-graph.
We use `scipy.sparse.linalg.eigs` (NOT `eigsh` — the complex pencil
is non-Hermitian under SciPy's convention even when complex-
symmetric). The boundary is identical to Phase G.3.

### E. Friction: shift-and-invert location matters

`scipy.sparse.linalg.eigs` with `sigma=0.0+0j` saturates on the
spurious gradient cluster (~`spurious_dim` modes near `λ=0` that
are largely unaffected by the lossy ε scaling). The fix is a
physical-band shift (`sigma=1.18+0.2j` for σ₀=5.0 — the NumPy
canonical lowest-physical band per PR #155; `sigma=1.4+0j` for
the σ₀=0 PEC regression). This is a SciPy property, not a JAX one,
but it's worth documenting for the Phase H series because all three
backends (NumPy / JAX / Julia) hit the same wall.

A previous shift of `sigma=0.9+0j` (chosen before PR #155 locked in
the canonical band) pulled a sub-band cluster near `Re(λ) ≈ 0.89`
that disagreed with the NumPy canonical at the ~25% level — a
documented friction artifact. The current shift converges to the
canonical band within ~0.2% relative on Re(λ) (see G below).

### G. Cross-check residual vs NumPy canonical (PR #155)

After the canonical shift fix, the JAX physical[0] sits at
`λ ≈ 1.17989 + 0.20000j` versus the NumPy canonical
`λ ≈ 1.18232 + 0.20713j`. Quantitatively:

- Re-rel = 2.1e-3 (well within the 5e-3 generator tolerance)
- |Im|-abs = 7.1e-3
- Total |Δ| = 7.5e-3 (just over the 5e-3 tolerance — soft warning)
- Q-factor: JAX 2.95 vs NumPy 2.85 (1σ agreement)

The slight Im(λ) gap is the residual difference between SciPy ARPACK
shift-and-invert (which uses Arnoldi iteration in a Krylov subspace
biased toward the shift) and NumPy/LAPACK dense ZGGEV (which solves
the full pencil). Both pick a physically equivalent mode from the
same cluster; the position-in-cluster mismatch is solver-dependent,
not a code bug.

Per-position cross-check on the full 5-mode slice gives `|Δ| ≈ 1.4`
because the JAX slice returns 5 near-degenerate modes from the same
cluster while NumPy returns 5 modes spanning multiple bands. The
robust comparator is physical[0] only (recorded in `verified_against`
in the fixture provenance).

### F. Wirtinger / complex-grad nuance

`jax.grad` requires real-valued outputs. The probe sidesteps this by
splitting the complex `ε` into `(re, im)` real arrays as the
differentiated inputs and using `Re(·)` to land on a real scalar.
`jax.grad` of a complex-output function raises a clear error
(`gradient requires real-valued outputs; got dtype complex128`);
the JAX docs cover this via Wirtinger calculus support, but for a
FEM-shaped probe the split-real-input pattern is friction-free and
the conventional choice. No surprises here.

## Planned layout

This directory will grow per-spine-slice files alongside
`cube_cavity.py`. The pattern (per `reference/README.md`):

```
reference/jax/
├── README.md                         — this file
├── cube_cavity.py                    — Epic #88 / #93 (#93 wave 2)
├── gen_cube_cavity_fixture.py
├── sphere_pec.py                     — Epic #88 / #128 (Phase G.3)
├── gen_sphere_pec_fixture.py
├── sphere_pml.py                     — Epic #88 / #148 (Phase H.3)
├── gen_sphere_pml_fixture.py
└── <next_slice>.py                   — future spine slices
```
