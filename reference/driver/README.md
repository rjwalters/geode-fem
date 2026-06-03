# `reference/driver/` — language-bridge drivers

Small driver scripts that consume a sidecar file produced by one
backend and close the spine at the boundary that backend can't
reach natively. Each script lives here (rather than under any
single backend) because it's the *seam* — the language-bridge —
that the friction-mining loop wants to highlight.

## `eigensolve_from_tfjava.py`

Picks up the fixture-schema JSON sidecar emitted by
`reference/tf_java/cube_cavity/CubeCavityMain` and runs the
SciPy generalized eigensolve on the reduced (K_int, M_int)
matrices. Emits a second fixture-schema JSON with the
eigenvalues.

Why a separate driver? TF-Java has no native sparse generalized
eigensolver. Per #93's acceptance criteria, delegating to a
Java ARPACK binding (`netlib-java`, `breeze`) or to SciPy is
acceptable — we picked SciPy because it's the same solver the
in-tree NumPy reference uses, ensuring an apples-to-apples
cross-check.

See `reference/tf_java/README.md` for the full TF-Java workflow.
