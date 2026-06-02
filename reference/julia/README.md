# Julia reference implementations

Complex-arithmetic reference backend per **Epic #88**. Probes
whether complex-arithmetic ergonomics surface different L4 friction
than the f64-pair representation used in Burn/NumPy.

## Status

Stub — concrete impls deferred to **#88 Phase E**.

## Planned layout

Mirrors `reference/numpy/`. Toolchain bootstrap will use Julia's
`Project.toml` / `Manifest.toml` for reproducibility; ARPACK access
goes through `Arpack.jl`.
