# JAX reference implementations

Differentiable reference backend per **Epic #88**. JAX probes how
natural the L4 calculus is for XLA tracing and whether autodiff
survives the assembly path.

## Status

Stub — concrete impls deferred to **#88 Phase C**. NumPy (Phase B)
must land first so JAX disagreements have an anchor.

## Planned layout

Mirrors `reference/numpy/`. See `reference/numpy/README.md` for the
invocation convention.
